use std::convert::TryFrom;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgSwapExactAmountIn, PoolParams, QueryPoolParamsRequest,
    QueryPoolParamsResponse, QueryTotalPoolLiquidityRequest, QueryTotalPoolLiquidityResponse,
    SwapAmountInRoute,
};

use cw_utils::Duration as CwDuration;

use apollo_proto_rust::osmosis::lockup::{MsgBeginUnlocking, MsgLockTokens};
use apollo_proto_rust::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_std::{
    from_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, Event, QuerierWrapper, QueryRequest,
    Response, StdError, StdResult, Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use cw_storage_plus::Item;
use cw_token::osmosis::OsmosisDenom;
use osmo_bindings::OsmosisQuery;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::utils::vec_into;
use crate::{CwDexError, Pool, Staking};

use super::helpers::{assert_native_coin, assert_only_native_coins};

/// Struct for interacting with Osmosis v1beta1 balancer pools. If `pool_id` maps to another type of pool this will fail.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisPool {
    /// The pool id of the pool to interact with
    pub pool_id: u64,
    // calcPoolOutGivenSingleIn - see here. Since all pools we are adding are 50/50, no need to store TotalWeight or the pool asset's weight
    // We should query this once Stargate queries are available
    // https://github.com/osmosis-labs/osmosis/blob/df2c511b04bf9e5783d91fe4f28a3761c0ff2019/x/gamm/pool-models/balancer/pool.go#L632
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

        let event = Event::new("apollo/cwdex/provide_liquidity")
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
        let token_out_mins =
            osmosis_calculate_exit_pool_amounts(querier, self.pool_id, lp_token.amount)?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender: recipient.to_string(),
                pool_id: self.pool_id,
                share_in_amount: lp_token.amount.to_string(),
                token_out_mins: vec_into(token_out_mins),
            }),
        };

        let event = Event::new("apollo/cwdex/withdraw_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("lp_token", lp_token.to_string())
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(exit_msg).add_event(event))
    }

    fn swap(
        &self,
        _deps: Deps,
        offer: Asset,
        ask: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let offer = assert_native_coin(&offer)?;
        let ask = assert_native_coin(&ask)?;

        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender: recipient.to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id: self.pool_id,
                    token_out_denom: ask.clone().denom,
                }],
                token_in: Some(offer.clone().into()),
                token_out_min_amount: ask.amount.to_string(),
            }),
        };

        let event = Event::new("apollo/cwdex/swap")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("offer", offer.to_string())
            .add_attribute("ask", ask.to_string())
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("token_out_min_amount", ask.amount.to_string());

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
        Ok(osmosis_calculate_exit_pool_amounts(querier, self.pool_id, asset.amount)?.into())
    }
}

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisStaking {
    /// Lockup duration in nano seconds. Allowed values 1 day, 1 week or 2 weeks.
    pub lockup_duration: u64,
}

impl OsmosisStaking {
    pub fn new(lockup_duration: u64) -> StdResult<Self> {
        if !(vec![86_400_000_000_000u64, 604800_000_000_000u64, 1209600_000_000_000u64]
            .contains(&lockup_duration))
        {
            return Err(StdError::generic_err("osmosis error: invalid lockup duration"));
        }
        Ok(Self {
            lockup_duration,
        })
    }
}

pub const LOCK_ID: Item<u64> = Item::new("lock_id"); // TODO: stargate query

impl Staking for OsmosisStaking {
    fn stake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let duration = Duration::from_nanos(self.lockup_duration);
        let asset = assert_native_coin(&asset)?;

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner: recipient.to_string(),
                duration: Some(apollo_proto_rust::google::protobuf::Duration {
                    seconds: i64::try_from(duration.as_secs())?,
                    nanos: duration.subsec_nanos() as i32,
                }),
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;
        let id = LOCK_ID.load(deps.storage)?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner: recipient.to_string(),
                id,
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, recipient: Addr) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        let std_duration: Duration = Duration::from_nanos(self.lockup_duration);
        Ok(CwDuration::Time(std_duration.as_secs()))
    }
}

/// Implementation of superfluid staking for osmosis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
}

impl Staking for OsmosisSuperfluidStaking {
    fn stake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;
        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender: recipient.to_string(),
                coins: vec![asset.into()],
                val_addr: self.validator_address.to_string(),
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, deps: Deps, _asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let lock_id = LOCK_ID.load(deps.storage)?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender: recipient.to_string(),
                lock_id,
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _recipient: Addr) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }

    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError> {
        todo!()
    }
}
