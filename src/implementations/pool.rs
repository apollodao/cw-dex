use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Deps, Env, MessageInfo};
use cw_asset::AssetInfo;
use std::str::FromStr;

use crate::error::CwDexError;
use crate::implementations::osmosis::OsmosisPool;
use crate::traits::pool::Pool as PoolTrait;

/// An enum with all known variants that implement the Pool trait.
/// The ideal solution would of course instead be to use a trait object so that
/// the caller can pass in any type that implements the Pool trait, but trait
/// objects require us not to implement the Sized trait, which cw_serde requires.
#[cw_serde]
#[derive(Copy)]
pub enum Pool {
    Osmosis(OsmosisPool),
}

impl Pool {
    pub fn as_trait(&self) -> Box<dyn PoolTrait> {
        match self {
            Pool::Osmosis(x) => Box::new(x.clone()),
        }
    }

    pub fn get_pool_for_lp_token(_deps: Deps, lp_token: &AssetInfo) -> Result<Self, CwDexError> {
        match lp_token {
            cw_asset::AssetInfoBase::Native(lp_token_denom) => {
                //The only Pool implementation that uses native denoms right now is Osmosis
                if !lp_token_denom.starts_with("gamm/pool/") {
                    return Err(CwDexError::NotLpToken {});
                }

                let pool_id_str =
                    lp_token_denom.strip_prefix("gamm/pool/").ok_or(CwDexError::NotLpToken {})?;

                let pool_id = u64::from_str(pool_id_str).map_err(|_| CwDexError::NotLpToken {})?;

                Ok(Pool::Osmosis(OsmosisPool {
                    pool_id,
                }))
            }
            _ => Err(CwDexError::NotLpToken {}), //TODO: Support Astroport, Junoswap, etc.
        }
    }
}

/// Implement the Pool trait for the Pool enum so we can use all the trait mehtods
/// directly on the enum type.
/// TODO: Use "enum_dispatch" macro instead? https://crates.io/crates/enum_dispatch
impl PoolTrait for Pool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        info: &MessageInfo,
        assets: cw_asset::AssetList,
        recipient: cosmwasm_std::Addr,
        slippage_tolerance: Option<cosmwasm_std::Decimal>,
    ) -> Result<cosmwasm_std::Response, CwDexError> {
        self.as_trait().provide_liquidity(deps, env, info, assets, recipient, slippage_tolerance)
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: cw_asset::Asset,
        recipient: cosmwasm_std::Addr,
    ) -> Result<cosmwasm_std::Response, CwDexError> {
        self.as_trait().withdraw_liquidity(deps, asset, recipient)
    }

    fn swap(
        &self,
        deps: Deps,
        offer_asset: cw_asset::Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: cosmwasm_std::Uint128,
        recipient: cosmwasm_std::Addr,
    ) -> Result<cosmwasm_std::Response, CwDexError> {
        self.as_trait().swap(deps, offer_asset, ask_asset_info, minimum_out_amount, recipient)
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<cw_asset::AssetList, CwDexError> {
        self.as_trait().get_pool_liquidity(deps)
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        asset: cw_asset::AssetList,
    ) -> Result<cw_asset::Asset, CwDexError> {
        self.as_trait().simulate_provide_liquidity(deps, asset)
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: cw_asset::Asset,
    ) -> Result<cw_asset::AssetList, CwDexError> {
        self.as_trait().simulate_withdraw_liquidity(deps, asset)
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: cw_asset::Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: cosmwasm_std::Uint128,
        sender: Option<String>,
    ) -> cosmwasm_std::StdResult<cosmwasm_std::Uint128> {
        self.as_trait().simulate_swap(deps, offer_asset, ask_asset_info, minimum_out_amount, sender)
    }
}
