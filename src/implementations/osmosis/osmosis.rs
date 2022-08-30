use std::convert::TryFrom;
use std::time::Duration;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgSwapExactAmountIn, SwapAmountInRoute,
};
use apollo_proto_rust::osmosis::lockup::{MsgBeginUnlocking, MsgLockTokens};
use apollo_proto_rust::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_std::{Addr, Coin, CosmosMsg, Decimal, Deps, Response, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetInfoBase, AssetList};
use cw_storage_plus::Item;
use cw_token::implementations::osmosis::OsmosisDenom;
use osmo_bindings::OsmosisQuery;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::utils::vec_into;
use crate::{CwDexError, Pool, Staking};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisPool {
    pub pool_id: u64,
    pub assets: Vec<String>,
    pub exit_fee: Decimal, // TODO: queriable? remove?
    pub swap_fee: Decimal,
    pub total_weight: Uint128,
    pub normalized_weight: Decimal,
    // calcPoolOutGivenSingleIn - see here. Since all pools we are adding are 50/50, no need to store TotalWeight or the pool asset's weight
    // We should query this once Stargate queries are available
    // https://github.com/osmosis-labs/osmosis/blob/df2c511b04bf9e5783d91fe4f28a3761c0ff2019/x/gamm/pool-models/balancer/pool.go#L632
}

pub struct OsmosisAssets {
    pub assets: Vec<AssetInfoBase<OsmosisDenom>>,
}

fn assert_only_native_coins(assets: AssetList) -> Result<Vec<Coin>, CwDexError> {
    assets.into_iter().map(assert_native_coin).collect::<Result<Vec<Coin>, CwDexError>>()
}

fn assert_native_coin(asset: &Asset) -> Result<Coin, CwDexError> {
    match asset.info {
        AssetInfoBase::Native(_) => asset.try_into().map_err(|e: StdError| e.into()),
        _ => Err(CwDexError::InvalidInAsset {
            a: asset.clone(),
        }),
    }
}

impl Pool<OsmosisQuery> for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        assets: AssetList,
    ) -> Result<CosmosMsg, CwDexError> {
        let assets = assert_only_native_coins(assets)?;
        let sender = VAULT_ADDR.load(deps.storage)?.to_string();

        let shares_out = osmosis_calculate_join_pool_shares(deps, self.pool_id, assets.to_vec())?;

        let join_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::JoinPool.to_string(),
            value: encode(MsgJoinPool {
                pool_id: self.pool_id,
                sender,
                share_out_amount: shares_out.amount.to_string(),
                token_in_maxs: assets
                    .into_iter()
                    .map(|coin| coin.into())
                    .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
            }),
        };

        Ok(join_msg)
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        asset: Asset,
    ) -> Result<CosmosMsg, CwDexError> {
        let lp_token = assert_native_coin(&asset)?;
        let sender = VAULT_ADDR.load(deps.storage)?.to_string();

        let token_out_mins = osmosis_calculate_exit_pool_amounts(
            deps,
            self.pool_id,
            lp_token.amount,
            self.exit_fee,
        )?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender,
                pool_id: self.pool_id,
                share_in_amount: lp_token.amount.to_string(),
                token_out_mins: vec_into(token_out_mins),
            }),
        };

        Ok(exit_msg)
    }

    fn swap(&self, deps: Deps, offer: Asset, ask: Asset) -> Result<CosmosMsg, CwDexError> {
        let offer = assert_native_coin(&offer)?;
        let ask = assert_native_coin(&ask)?;
        let sender = VAULT_ADDR.load(deps.storage)?.to_string();

        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender,
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

    fn get_pool_assets(&self) -> Result<AssetList, CwDexError> {
        Ok(self
            .assets
            .iter()
            .map(|asset| {
                Coin {
                    denom: asset.clone(),
                    amount: Uint128::zero(),
                }
                .into()
            })
            .collect::<Vec<Asset>>()
            .into())
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        Ok(osmosis_calculate_join_pool_shares(
            deps,
            self.pool_id,
            assert_only_native_coins(assets)?,
        )?
        .into())
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps<OsmosisQuery>,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        Ok(osmosis_calculate_exit_pool_amounts(deps, self.pool_id, asset.amount, self.exit_fee)?
            .into())
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

pub const LOCK_ID: Item<u64> = Item::new("lock_id");
pub const VAULT_ADDR: Item<Addr> = Item::<Addr>::new("vault_addr");

impl Staking for OsmosisStaking {
    fn stake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError> {
        let duration = Duration::from_nanos(self.lockup_duration);
        let asset = assert_native_coin(&asset)?;
        let owner = VAULT_ADDR.load(deps.storage)?.to_string();

        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::BondLP.to_string(),
            value: encode(MsgLockTokens {
                owner,
                duration: Some(apollo_proto_rust::google::protobuf::Duration {
                    seconds: i64::try_from(duration.as_secs())?,
                    nanos: duration.subsec_nanos() as i32,
                }),
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;
        let id = LOCK_ID.load(deps.storage)?;
        let owner = VAULT_ADDR.load(deps.storage)?.to_string();

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::UnBondLP.to_string(),
            value: encode(MsgBeginUnlocking {
                owner,
                id,
                coins: vec![asset.into()],
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}

/// Implementation of superfluid staking for osmosis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
}

impl Staking for OsmosisSuperfluidStaking {
    fn stake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError> {
        let asset = assert_native_coin(&asset)?;
        let sender = VAULT_ADDR.load(deps.storage)?.to_string();
        let stake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidBondLP.to_string(),
            value: encode(MsgLockAndSuperfluidDelegate {
                sender,
                coins: vec![asset.into()],
                val_addr: self.validator_address.to_string(),
            }),
        };

        Ok(Response::new().add_message(stake_msg))
    }

    fn unstake(&self, deps: Deps, _asset: Asset) -> Result<Response, CwDexError> {
        let lock_id = LOCK_ID.load(deps.storage)?;
        let sender = VAULT_ADDR.load(deps.storage)?.to_string();

        let unstake_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SuperfluidUnBondLP.to_string(),
            value: encode(MsgSuperfluidUnbondLock {
                sender,
                lock_id,
            }),
        };

        Ok(Response::new().add_message(unstake_msg))
    }

    fn claim_rewards(&self) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        Ok(Response::new())
    }
}
