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
use cosmwasm_std::{Addr, Coin, CosmosMsg, Response, StdError, StdResult, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CwDexError, Pool, Staking};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisPool {
    pool_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisOptions {
    sender: Addr,
    share_out_amount: Option<Uint128>,
    token_out_mins: Option<Vec<Coin>>,
}

impl Pool<OsmosisOptions, Coin> for OsmosisPool {
    fn provide_liquidity(
        &self,
        assets: Vec<Coin>,
        options: OsmosisOptions,
    ) -> Result<CosmosMsg, CwDexError> {
        let share_out_amount = options.share_out_amount.ok_or(CwDexError::Std(
            StdError::generic_err("Osmosis error: share_out_amount not provided."),
        ))?;

        let join_msg = if assets.len() == 1 {
            let coin_in = assets[0].clone();
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinSwapExternAmountIn.to_string(),
                value: encode(MsgJoinSwapExternAmountIn {
                    sender: options.sender.to_string(),
                    pool_id: self.pool_id,
                    token_in: Some(coin_in.into()),
                    share_out_min_amount: share_out_amount.to_string(),
                }),
            }
        } else {
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinPool.to_string(),
                value: encode(MsgJoinPool {
                    pool_id: self.pool_id,
                    sender: options.sender.to_string(),
                    share_out_amount: share_out_amount.to_string(),
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
        asset: Coin,
        options: OsmosisOptions,
    ) -> Result<CosmosMsg, CwDexError> {
        let token_out_mins = options.token_out_mins.ok_or(CwDexError::Std(
            StdError::generic_err("Osmosis error: token_out_mins not provided."),
        ))?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender: options.sender.to_string(),
                pool_id: self.pool_id,
                share_in_amount: asset.amount.to_string(),
                token_out_mins: token_out_mins
                    .into_iter()
                    .map(|coin| coin.into())
                    .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
            }),
        };

        Ok(exit_msg)
    }

    fn swap(
        &self,
        offer: Coin,
        ask: Coin,
        options: OsmosisOptions,
    ) -> Result<CosmosMsg, CwDexError> {
        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender: options.sender.to_string(),
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisStakingOptions {
    owner: Addr,
    lockup_id: Option<u64>,
}

impl Staking<OsmosisStakingOptions, Coin> for OsmosisStaking {
    fn stake(&self, asset: Coin, options: OsmosisStakingOptions) -> Result<Response, CwDexError> {
        let duration = Duration::from_nanos(self.lockup_duration);

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner: options.owner.to_string(),
                duration: Some(apollo_proto_rust::google::protobuf::Duration {
                    seconds: i64::try_from(duration.as_secs())?,
                    nanos: duration.subsec_nanos() as i32,
                }),
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, asset: Coin, options: OsmosisStakingOptions) -> Result<Response, CwDexError> {
        let id = options
            .lockup_id
            .ok_or(CwDexError::Std(StdError::generic_err("Osmosis: lockup_id not provided")))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner: options.owner.to_string(),
                id,
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _stake_info: OsmosisStakingOptions) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}

/// Implementation of superfluid staking for osmosis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
}

impl Staking<OsmosisStakingOptions, Coin> for OsmosisSuperfluidStaking {
    fn stake(&self, asset: Coin, options: OsmosisStakingOptions) -> Result<Response, CwDexError> {
        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender: options.owner.to_string(),
                coins: vec![asset.into()],
                val_addr: self.validator_address.to_string(),
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(
        &self,
        _asset: Coin,
        options: OsmosisStakingOptions,
    ) -> Result<Response, CwDexError> {
        let lock_id = options
            .lockup_id
            .ok_or(CwDexError::Std(StdError::generic_err("Osmosis: lockup_id not provided")))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender: options.owner.to_string(),
                lock_id,
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _claim_options: OsmosisStakingOptions) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}
