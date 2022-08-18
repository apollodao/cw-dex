use std::convert::TryFrom;
use std::time::Duration;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgJoinSwapExternAmountIn, MsgSwapExactAmountIn, SwapAmountInRoute,
};
use apollo_proto_rust::osmosis::lockup::{MsgBeginUnlocking, MsgLockTokens};
use apollo_proto_rust::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_std::{
    Addr, Coin, CosmosMsg, Decimal, Deps, Empty, MessageInfo, Response, StdError, StdResult,
    Uint128,
};
use cw_asset::osmosis::OsmosisDenom;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use osmo_bindings::{OsmosisQuerier, OsmosisQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use crate::osmosis::osmosis_math::{
    calculate_exit_pool_amounts_osmosis, calculate_join_pool_shares_osmosis,
};
use crate::utils::vec_into;
use crate::{CwDexError, Pool, Staking};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisPool {
    pool_id: u64,
    assets: Vec<String>,
    exit_fee: Decimal, // TODO: queriable? remove?
    swap_fee: Decimal,
    total_weight: Uint128,
    normalized_weight: Decimal,
    // calcPoolOutGivenSingleIn - see here. Since all pools we are adding are 50/50, no need to store TotalWeight or the pool asset's weight
    // We should query this once Stargate queries are available
    // https://github.com/osmosis-labs/osmosis/blob/df2c511b04bf9e5783d91fe4f28a3761c0ff2019/x/gamm/pool-models/balancer/pool.go#L632
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisOptions {
    sender: Addr,
    lockup_id: Option<u64>,
}

pub struct OsmosisAssets {
    pub assets: Vec<AssetInfoBase<OsmosisDenom>>,
}

impl Pool<OsmosisQuery, Coin> for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        info: &MessageInfo,
        assets: Vec<Coin>,
    ) -> Result<CosmosMsg, CwDexError> {
        let shares_out = calculate_join_pool_shares_osmosis(
            deps,
            self.pool_id,
            (&assets).into(),
            self.total_weight,
            self.normalized_weight,
            self.swap_fee,
        )?;

        let join_msg = if assets.len() == 1 {
            let coin_in = assets[0].clone();
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinSwapExternAmountIn.to_string(),
                value: encode(MsgJoinSwapExternAmountIn {
                    sender: info.sender.to_string(),
                    pool_id: self.pool_id,
                    token_in: Some(coin_in.into()),
                    share_out_min_amount: shares_out.amount.to_string(),
                }),
            }
        } else {
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinPool.to_string(),
                value: encode(MsgJoinPool {
                    pool_id: self.pool_id,
                    sender: info.sender.to_string(),
                    share_out_amount: shares_out.amount.to_string(),
                    token_in_maxs: assets
                        .into_iter()
                        .map(|coin| coin.into())
                        .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
                }),
            }
        };

        Ok(join_msg)
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        info: &MessageInfo,
        asset: Coin,
        asset_to_withdraw: Option<Coin>,
    ) -> Result<CosmosMsg, CwDexError> {
        let token_out_mins = calculate_exit_pool_amounts_osmosis(
            deps,
            self.pool_id,
            asset.amount,
            self.exit_fee,
            self.swap_fee,
            self.normalized_weight,
            self.total_weight,
            asset_to_withdraw,
        )?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender: info.sender.to_string(),
                pool_id: self.pool_id,
                share_in_amount: asset.amount.to_string(),
                token_out_mins: vec_into(token_out_mins),
            }),
        };

        Ok(exit_msg)
    }

    fn swap(&self, info: &MessageInfo, offer: Coin, ask: Coin) -> Result<CosmosMsg, CwDexError> {
        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender: info.sender.to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id: self.pool_id,
                    token_out_denom: ask.denom,
                }],
                token_in: Some(offer.into()),
                token_out_min_amount: ask.amount.to_string(),
            }),
        };

        Ok(swap_msg)
    }

    fn get_pool_assets(&self) -> Result<Vec<Coin>, CwDexError> {
        Ok(self
            .assets
            .iter()
            .map(|asset| Coin {
                denom: asset.clone(),
                amount: Uint128::zero(),
            })
            .collect())
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        asset: Vec<Coin>,
    ) -> Result<Coin, CwDexError> {
        Ok(calculate_join_pool_shares_osmosis(
            deps,
            self.pool_id,
            (&asset).into(),
            self.total_weight,
            self.normalized_weight,
            self.swap_fee,
        )?)
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        asset: Coin,
        asset_to_withdraw: Option<Coin>,
    ) -> Result<Vec<Coin>, CwDexError> {
        Ok(calculate_exit_pool_amounts_osmosis(
            deps,
            self.pool_id,
            asset.amount,
            self.exit_fee,
            self.swap_fee,
            self.normalized_weight,
            self.total_weight,
            asset_to_withdraw,
        )?)
    }
}

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisStaking {
    /// Lockup duration in nano seconds. Allowed values 1 day, 1 week or 2 weeks.
    lockup_duration: u64,
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

impl Staking<OsmosisOptions, Coin> for OsmosisStaking {
    fn stake(&self, asset: Coin, options: OsmosisOptions) -> Result<Response, CwDexError> {
        let duration = Duration::from_nanos(self.lockup_duration);

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner: options.sender.to_string(),
                duration: Some(apollo_proto_rust::google::protobuf::Duration {
                    seconds: i64::try_from(duration.as_secs())?,
                    nanos: duration.subsec_nanos() as i32,
                }),
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, asset: Coin, options: OsmosisOptions) -> Result<Response, CwDexError> {
        let id = options
            .lockup_id
            .ok_or(CwDexError::Std(StdError::generic_err("Osmosis: lockup_id not provided")))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner: options.sender.to_string(),
                id,
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _stake_info: OsmosisOptions) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}

/// Implementation of superfluid staking for osmosis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
}

impl Staking<OsmosisOptions, Coin> for OsmosisSuperfluidStaking {
    fn stake(&self, asset: Coin, options: OsmosisOptions) -> Result<Response, CwDexError> {
        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender: options.sender.to_string(),
                coins: vec![asset.into()],
                val_addr: self.validator_address.to_string(),
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, _asset: Coin, options: OsmosisOptions) -> Result<Response, CwDexError> {
        let lock_id = options
            .lockup_id
            .ok_or(CwDexError::Std(StdError::generic_err("Osmosis: lockup_id not provided")))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender: options.sender.to_string(),
                lock_id,
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _claim_options: OsmosisOptions) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}
