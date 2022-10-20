use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinSwapExternAmountIn, MsgSwapExactAmountIn, QuerySwapExactAmountInRequest,
    QuerySwapExactAmountInResponse, QueryTotalPoolLiquidityRequest,
    QueryTotalPoolLiquidityResponse, SwapAmountInRoute,
};

use cw_utils::Duration as CwDuration;

use apollo_proto_rust::osmosis::lockup::{MsgBeginUnlocking, MsgForceUnlock, MsgLockTokens};
use apollo_proto_rust::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Coin, CosmosMsg, Decimal, Deps, Event, QuerierWrapper, QueryRequest, ReplyOn, Response,
    StdError, StdResult, SubMsg, Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use osmo_bindings::OsmosisQuery;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::utils::vec_into;
use crate::{CwDexError, Lockup, Pool, Staking};

use super::helpers::{
    assert_native_asset_info, assert_native_coin, assert_only_native_coins, merge_assets,
    ToProtobufDuration,
};

/// Struct for interacting with Osmosis v1beta1 balancer pools. If `pool_id` maps to another type of pool this will fail.
#[cw_serde]
#[derive(Copy)]
pub struct OsmosisPool {
    /// The pool id of the pool to interact with
    pub pool_id: u64,
    // calcPoolOutGivenSingleIn - see here. Since all pools we are adding are 50/50, no need to store TotalWeight or the pool asset's weight
    // We should query this once Stargate queries are available
    // https://github.com/osmosis-labs/osmosis/blob/df2c511b04bf9e5783d91fe4f28a3761c0ff2019/x/gamm/pool-models/balancer/pool.go#L632
}

impl OsmosisPool {
    pub fn new(pool_id: u64) -> Self {
        Self {
            pool_id,
        }
    }
}

impl Pool for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let mut assets = assets;

        // Remove all zero amount Coins, merge duplicates and assert that all assets are native.
        let assets = assert_only_native_coins(merge_assets(assets.purge().deref())?)?;

        // Construct osmosis querier
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        let slippage_tolerance =
            Decimal::one() - slippage_tolerance.unwrap_or_else(|| Decimal::one());

        // TODO: Provide liquidity double sided.
        // For now we only provide liquidity single sided since the ratio of the underlying tokens
        // needs to be exactly the same as the the pool ratio otherwise the remainder is returned
        // and there are no queries yet
        let (join_msgs, shares_out): (Vec<CosmosMsg>, Vec<Uint128>) = assets
            .into_iter()
            .map(|coin| {
                // TODO: Turn into stargate query
                let shares_out_min = slippage_tolerance
                    * osmosis_calculate_join_pool_shares(
                        querier,
                        self.pool_id,
                        vec![coin.clone()],
                    )?
                    .amount;
                Ok((
                    CosmosMsg::Stargate {
                        type_url: OsmosisTypeURLs::JoinSwapExternAmountIn.to_string(),
                        value: encode(MsgJoinSwapExternAmountIn {
                            sender: recipient.to_string(),
                            pool_id: self.pool_id,
                            token_in: Some(coin.into()),
                            share_out_min_amount: shares_out_min.to_string(),
                        }),
                    },
                    shares_out_min,
                ))
            })
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .unzip();

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("minimum_shares_out", shares_out.iter().sum::<Uint128>().to_string())
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_messages(join_msgs).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let lp_token = assert_native_coin(&asset)?;

        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        // TODO: Query for exit pool amounts?
        let token_out_mins = osmosis_calculate_exit_pool_amounts(querier, self.pool_id, &lp_token)?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender: recipient.to_string(),
                pool_id: self.pool_id,
                share_in_amount: lp_token.amount.to_string(),
                token_out_mins: vec_into(token_out_mins),
            }),
        };

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("lp_token", lp_token.to_string())
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(exit_msg).add_event(event))
    }

    fn swap(
        &self,
        _deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let offer = assert_native_coin(&offer_asset)?;
        let ask_denom = assert_native_asset_info(&ask_asset_info)?;

        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender: recipient.to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id: self.pool_id,
                    token_out_denom: ask_denom.clone(),
                }],
                token_in: Some(offer.clone().into()),
                token_out_min_amount: minimum_out_amount.to_string(),
            }),
        };

        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("offer", offer.to_string())
            .add_attribute("ask", ask_denom)
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("token_out_min_amount", minimum_out_amount);

        Ok(Response::new().add_message(swap_msg).add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let pool_assets =
            deps.querier.query::<QueryTotalPoolLiquidityResponse>(&QueryRequest::Stargate {
                path: OsmosisTypeURLs::QueryTotalPoolLiquidity.to_string(),
                data: encode(QueryTotalPoolLiquidityRequest {
                    pool_id: self.pool_id,
                }),
            })?;

        let asset_list: AssetList = pool_assets
            .liquidity
            .into_iter()
            .map(|coin| {
                Ok(Asset {
                    info: AssetInfo::Native(coin.denom),
                    amount: Uint128::from_str(&coin.amount)?,
                })
            })
            .collect::<StdResult<Vec<Asset>>>()?
            .into();

        Ok(asset_list)
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());
        Ok(osmosis_calculate_join_pool_shares(
            querier,
            self.pool_id,
            assert_only_native_coins(assets)?,
        )?
        .into())
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());
        let lp_token = assert_native_coin(&asset)?;
        Ok(osmosis_calculate_exit_pool_amounts(querier, self.pool_id, &lp_token)?.into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer: Asset,
        _ask_asset_info: AssetInfo,
        _minimum_out_amount: Uint128,
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        let offer: Coin = offer.try_into()?;
        let swap_response =
            deps.querier.query::<QuerySwapExactAmountInResponse>(&QueryRequest::Stargate {
                path: OsmosisTypeURLs::QuerySwapExactAmountIn.to_string(),
                data: encode(QuerySwapExactAmountInRequest {
                    sender: sender
                        .ok_or(StdError::generic_err("sender is required for osmosis"))?,
                    pool_id: self.pool_id,
                    routes: vec![SwapAmountInRoute {
                        pool_id: self.pool_id,
                        token_out_denom: offer.denom.clone(),
                    }],
                    token_in: offer.to_string(),
                }),
            })?;
        Uint128::from_str(swap_response.token_out_amount.as_str())
    }
}

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[cw_serde]
pub struct OsmosisStaking {
    /// Lockup duration in nano seconds. Allowed values 1 day, 1 week or 2 weeks.
    pub lockup_duration: Duration,

    pub lock_id: Option<u64>,
}

impl OsmosisStaking {
    /// Creates a new OsmosisStaking instance with lock up duration set to `lockup_duration`.
    ///
    /// Arguments:
    /// - `lockup_duration` is the duration of the lockup period in seconds.
    ///
    /// Returns an error if `lockup_duration` is not one of the allowed values,
    /// 86400, 604800 or 1209600, representing 1 day, 1 week or 2 weeks respectively.
    pub fn new(lockup_duration: u64, lock_id: Option<u64>) -> StdResult<Self> {
        if !(vec![86400u64, 604800u64, 1209600u64].contains(&lockup_duration)) {
            return Err(StdError::generic_err("osmosis error: invalid lockup duration"));
        }
        Ok(Self {
            lockup_duration: Duration::from_secs(lockup_duration),
            lock_id,
        })
    }
}

pub const OSMOSIS_LOCK_TOKENS_REPLY_ID: u64 = 123;
pub const OSMOSIS_UNLOCK_TOKENS_REPLY_ID: u64 = 124;

impl Staking for OsmosisStaking {
    fn stake(&self, _deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner: recipient.to_string(),
                duration: Some(self.lockup_duration.to_protobuf_duration()),
                coins: vec![asset.clone().into()],
            }),
        };

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("lockup_duration_secs", self.lockup_duration.as_secs().to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_LOCK_TOKENS_REPLY_ID,
                msg: stake_msg,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }

    fn unstake(&self, _deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;

        let id = self.lock_id.ok_or(StdError::generic_err("osmosis error: lock id not set"))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner: recipient.to_string(),
                id,
                coins: vec![asset.clone().into()],
            }),
        };

        let event = Event::new("apollo/cw-dex/unstake")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("lockup_duration_secs", self.lockup_duration.as_secs().to_string())
            .add_attribute("lock_id", id.to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_UNLOCK_TOKENS_REPLY_ID,
                msg: unstake_msg,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }

    fn claim_rewards(&self, _recipient: Addr) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        let event =
            Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "osmosis_staking");
        Ok(Response::new().add_event(event))
    }
}

impl Lockup for OsmosisStaking {
    fn force_unlock(
        &self,
        _deps: Deps,
        lockup_id: Option<u64>,
        assets: AssetList,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let lockup_id = match lockup_id {
            Some(id) => Ok(id),
            None => self.lock_id.ok_or(StdError::generic_err("osmosis error: lock id not set")),
        }?;

        let coins_to_unlock =
            assets.into_iter().map(|a| a.try_into()).collect::<StdResult<Vec<Coin>>>()?;

        let force_unlock_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ForceUnlock.to_string(),
            value: encode(MsgForceUnlock {
                owner: recipient.to_string(),
                id: lockup_id,
                coins: coins_to_unlock.into_iter().map(|c| c.into()).collect(),
            }),
        };

        let event = Event::new("apollo/cw-dex/force-unlock")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("lockup_id", lockup_id.to_string());

        Ok(Response::new().add_message(force_unlock_msg).add_event(event))
    }

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        Ok(CwDuration::Time(self.lockup_duration.as_secs()))
    }
}

/// Implementation of superfluid staking for osmosis.
#[cw_serde]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
    lock_id: Option<u64>,
}

const TWO_WEEKS_IN_SECS: u64 = 14 * 24 * 60 * 60;

impl Staking for OsmosisSuperfluidStaking {
    fn stake(&self, _deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;
        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender: recipient.to_string(),
                coins: vec![asset.clone().into()],
                val_addr: self.validator_address.to_string(),
            }),
        };

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "osmosis_superfluid_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("validator_address", self.validator_address.to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_LOCK_TOKENS_REPLY_ID,
                msg: stake_msg,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }

    fn unstake(&self, _deps: Deps, _asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let lock_id =
            self.lock_id.ok_or(StdError::generic_err("osmosis error: lock id not set"))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender: recipient.to_string(),
                lock_id,
            }),
        };

        let event = Event::new("apollo/cw-dex/unstake")
            .add_attribute("type", "osmosis_superfluid_staking")
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("validator_address", self.validator_address.to_string())
            .add_attribute("lock_id", lock_id.to_string());

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }

    fn claim_rewards(&self, _recipient: Addr) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        let event = Event::new("apollo/cw-dex/claim_rewards")
            .add_attribute("type", "osmosis_superfluid_staking");
        Ok(Response::new().add_event(event))
    }
}

impl Lockup for OsmosisSuperfluidStaking {
    fn force_unlock(
        &self,
        _deps: Deps,
        _lockup_id: Option<u64>,
        _assets: AssetList,
        _recipient: Addr,
    ) -> Result<Response, CwDexError> {
        unimplemented!()
    }

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        // Lockup time for superfluid staking is always 14 days.
        Ok(CwDuration::Time(TWO_WEEKS_IN_SECS))
    }
}
