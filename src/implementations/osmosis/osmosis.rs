use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgSwapExactAmountIn, QueryTotalPoolLiquidityRequest,
    QueryTotalPoolLiquidityResponse, SwapAmountInRoute,
};

use cw_utils::Duration as CwDuration;

use apollo_proto_rust::osmosis::lockup::{MsgBeginUnlocking, MsgLockTokens};
use apollo_proto_rust::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, CosmosMsg, Deps, Event, QuerierWrapper, QueryRequest, Response, StdError, StdResult,
    Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use osmo_bindings::OsmosisQuery;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::utils::vec_into;
use crate::{CwDexError, Pool, Staking};

use super::helpers::{
    assert_native_asset_info, assert_native_coin, assert_only_native_coins, query_lock,
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
    ) -> Result<Response, CwDexError> {
        let assets = assert_only_native_coins(assets)?;

        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        // TODO: Turn into stargate query
        let shares_out =
            osmosis_calculate_join_pool_shares(querier, self.pool_id, assets.to_vec())?;

        let join_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::JoinPool.to_string(),
            value: encode(MsgJoinPool {
                pool_id: self.pool_id,
                sender: recipient.to_string(),
                share_out_amount: shares_out.amount.to_string(),
                token_in_maxs: assets
                    .into_iter()
                    .map(|coin| coin.into())
                    .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
            }),
        };

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("shares_out", shares_out.to_string())
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(join_msg).add_event(event))
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
        _deps: Deps,
        _offer_asset: Asset,
        _ask_asset_info: AssetInfo,
        _minimum_out_amount: Uint128,
    ) -> Result<Uint128, CwDexError> {
        // TODO: How do we do this? I don't see a stargate query for it...
        todo!()
    }
}

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[cw_serde]
pub struct OsmosisStaking {
    /// Lockup duration in nano seconds. Allowed values 1 day, 1 week or 2 weeks.
    pub lockup_duration: Duration,
}

impl OsmosisStaking {
    /// Creates a new OsmosisStaking instance with lock up duration set to `lockup_duration`.
    ///
    /// Arguments:
    /// - `lockup_duration` is the duration of the lockup period in seconds.
    ///
    /// Returns an error if `lockup_duration` is not one of the allowed values,
    /// 86400, 604800 or 1209600, representing 1 day, 1 week or 2 weeks respectively.
    pub fn new(lockup_duration: u64) -> StdResult<Self> {
        if !(vec![86400u64, 604800u64, 1209600u64].contains(&lockup_duration)) {
            return Err(StdError::generic_err("osmosis error: invalid lockup duration"));
        }
        Ok(Self {
            lockup_duration: Duration::from_secs(lockup_duration),
        })
    }
}

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

        Ok(Response::new().add_message(stake_msg).add_event(event))
    }

    fn unstake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;

        let id = query_lock(deps.querier, &recipient, self.lockup_duration)?.id;

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

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }

    fn claim_rewards(&self, _recipient: Addr) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        let event =
            Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "osmosis_staking");
        Ok(Response::new().add_event(event))
    }

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        Ok(CwDuration::Time(self.lockup_duration.as_secs()))
    }
}

/// Implementation of superfluid staking for osmosis.
#[cw_serde]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
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

        Ok(Response::new().add_message(stake_msg).add_event(event))
    }

    fn unstake(&self, deps: Deps, _asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let lock_id =
            query_lock(deps.querier, &recipient, Duration::from_secs(TWO_WEEKS_IN_SECS))?.id;

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

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        // Lockup time for superfluid staking is always 14 days.
        Ok(CwDuration::Time(TWO_WEEKS_IN_SECS))
    }
}
