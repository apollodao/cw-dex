use std::convert::{TryFrom, TryInto};
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
use cosmwasm_std::{Addr, Coin, CosmosMsg, Empty, Response, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CwDexError, Pool, Staking};

// TODO: How do we best handle the sender field? Only needed for osmosis.
//       Use generic in trait to hold osmo specific info?

pub struct OsmosisPool {
    pool_id: u64,
}

pub struct OsmosisProvideLiquidityOptions {
    sender: Addr,
    share_out_amount: Uint128,
}

pub struct OsmosisWithdrawLiquidityOptions {
    sender: Addr,
    token_out_mins: Vec<Coin>,
}

pub struct OsmosisSwapOptions {
    pub sender: Addr,
}

impl Pool<OsmosisProvideLiquidityOptions, OsmosisWithdrawLiquidityOptions, OsmosisSwapOptions>
    for OsmosisPool
{
    fn provide_liquidity(
        &self,
        assets: AssetList,
        provide_liquidity_options: Option<OsmosisProvideLiquidityOptions>, // TODO: Make non optional?
    ) -> Result<CosmosMsg, CwDexError> {
        let coins =
            assets.into_iter().map(|a| Coin::try_from(a)).collect::<StdResult<Vec<Coin>>>()?;

        let options = provide_liquidity_options.ok_or(CwDexError::Std(StdError::generic_err(
            "osmosis error: provide liquidity options",
        )))?;

        let join_msg = if coins.len() == 1 {
            let coin_in = coins[0].clone();
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinSwapExternAmountIn.to_string(),
                value: encode(MsgJoinSwapExternAmountIn {
                    sender: options.sender.to_string(),
                    pool_id: self.pool_id,
                    token_in: Some(coin_in.into()),
                    share_out_min_amount: options.share_out_amount.to_string(),
                }),
            }
        } else {
            CosmosMsg::Stargate {
                type_url: OsmosisTypeURLs::JoinPool.to_string(),
                value: encode(MsgJoinPool {
                    pool_id: self.pool_id,
                    sender: options.sender.to_string(),
                    share_out_amount: options.share_out_amount.to_string(),
                    token_in_maxs: coins
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
        asset: Asset,
        withdraw_liquidity_options: Option<OsmosisWithdrawLiquidityOptions>, // TODO: Make non optional?
    ) -> Result<CosmosMsg, CwDexError> {
        let options = withdraw_liquidity_options.ok_or(CwDexError::Std(StdError::generic_err(
            "osmosis error: no withdraw liquidity options",
        )))?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender: options.sender.to_string(),
                pool_id: self.pool_id,
                share_in_amount: asset.amount.to_string(),
                token_out_mins: options
                    .token_out_mins
                    .into_iter()
                    .map(|coin| coin.into())
                    .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
            }),
        };

        Ok(exit_msg)
    }

    fn swap(
        &self,
        offer: Asset,
        ask: Asset,
        swap_options: Option<OsmosisSwapOptions>, // TODO: Make non optional?
    ) -> Result<CosmosMsg, CwDexError> {
        let out_denom = match ask.info {
            AssetInfo::Cw20(_) => Err(CwDexError::InvalidOutAsset {}),
            AssetInfo::Native(denom) => Ok(denom),
        }?;

        let sender = swap_options
            .ok_or(CwDexError::Std(StdError::generic_err("osmosis error: no swap options")))?
            .sender
            .to_string();

        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender,
                routes: vec![SwapAmountInRoute {
                    pool_id: self.pool_id,
                    token_out_denom: out_denom,
                }],
                token_in: Some(Coin::try_from(offer)?.into()),
                token_out_min_amount: ask.amount.to_string(),
            }),
        };

        Ok(swap_msg)
    }
}

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
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

pub struct OsmosisStakeOptions {
    owner: Addr,
}

pub struct OsmosisUnstakeOptions {
    owner: Addr,
    lockup_id: u64,
}

impl Staking<OsmosisStakeOptions, OsmosisUnstakeOptions> for OsmosisStaking {
    fn stake(
        &self,
        amount: Asset,
        stake_info: Option<OsmosisStakeOptions>,
    ) -> Result<Response, CwDexError> {
        let stake_info = stake_info
            .ok_or(CwDexError::Std(StdError::generic_err("osmosis stake: no stake info")))?;
        let owner = stake_info.owner.to_string();

        let duration = Duration::from_nanos(self.lockup_duration);
        let coin: Coin = amount.try_into()?;

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner,
                duration: Some(apollo_proto_rust::google::protobuf::Duration {
                    seconds: i64::try_from(duration.as_secs())?,
                    nanos: duration.subsec_nanos() as i32,
                }),
                coins: vec![apollo_proto_rust::cosmos::base::v1beta1::Coin {
                    denom: coin.denom.to_string(),
                    amount: coin.amount.to_string(),
                }],
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(
        &self,
        amount: Asset,
        unstake_options: Option<OsmosisUnstakeOptions>,
    ) -> Result<Response, CwDexError> {
        let unstake_options = unstake_options
            .ok_or(CwDexError::Std(StdError::generic_err("osmosis unstake: no unstake options")))?;
        let owner = unstake_options.owner.to_string();
        let id = unstake_options.lockup_id;

        let coin: Coin = amount.try_into()?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner,
                id,
                coins: vec![apollo_proto_rust::cosmos::base::v1beta1::Coin {
                    denom: coin.denom.to_string(),
                    amount: coin.amount.to_string(),
                }],
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _stake_info: Option<Empty>) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}

/// Implementation of superfluid staking for osmosis.
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
}

impl Staking<OsmosisStakeOptions, OsmosisUnstakeOptions> for OsmosisSuperfluidStaking {
    fn stake(
        &self,
        amount: Asset,
        stake_options: Option<OsmosisStakeOptions>,
    ) -> Result<Response, CwDexError> {
        let stake_options = stake_options.ok_or(CwDexError::Std(StdError::generic_err(
            "osmosis superfluid stake: no stake options",
        )))?;

        let coin: Coin = amount.try_into()?;

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender: stake_options.owner.to_string(),
                coins: vec![apollo_proto_rust::cosmos::base::v1beta1::Coin {
                    denom: coin.denom.to_string(),
                    amount: coin.amount.to_string(),
                }],
                val_addr: self.validator_address.to_string(),
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(
        &self,
        _amount: Asset,
        unstake_options: Option<OsmosisUnstakeOptions>,
    ) -> Result<Response, CwDexError> {
        let unstake_options = unstake_options.ok_or(CwDexError::Std(StdError::generic_err(
            "osmosis superfluid unstake: no unstake options",
        )))?;

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender: unstake_options.owner.to_string(),
                lock_id: unstake_options.lockup_id,
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self, _claim_options: Option<Empty>) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}
