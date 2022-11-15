//! Contains an enum with variants for Pool implementations.
//! For use in serialization.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Deps, Env, Response, StdResult, Uint128};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use std::str::FromStr;

use crate::error::CwDexError;
use crate::implementations::osmosis::OsmosisPool;
use crate::junoswap::JunoswapPool;
use crate::traits::pool::Pool as PoolTrait;

/// An enum with all known variants that implement the Pool trait.
/// The ideal solution would of course instead be to use a trait object so that
/// the caller can pass in any type that implements the Pool trait, but trait
/// objects require us not to implement the Sized trait, which cw_serde requires.
#[cw_serde]
pub enum Pool {
    /// Contains an Osmosis pool implementation
    Osmosis(OsmosisPool),
    /// Contains an Junoswap pool implementation
    Junoswap(JunoswapPool),
}

impl Pool {
    /// Returns a specific `Pool` instance as a boxed generic `Pool` trait
    pub fn as_trait(&self) -> Box<dyn PoolTrait> {
        match self {
            Pool::Osmosis(x) => Box::new(x.clone()),
            Pool::Junoswap(x) => Box::new(x.clone()),
        }
    }

    /// Returns the matching pool given a LP token.
    ///
    /// Arguments:
    /// - `lp_token`: Said LP token
    pub fn get_pool_for_lp_token(_deps: Deps, lp_token: &AssetInfo) -> Result<Self, CwDexError> {
        match lp_token {
            AssetInfoBase::Native(lp_token_denom) => {
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

// Implement the Pool trait for the Pool enum so we can use all the trait methods
// directly on the enum type.
// TODO: Use "enum_dispatch" macro instead? https://crates.io/crates/enum_dispatch
impl PoolTrait for Pool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        self.as_trait().provide_liquidity(deps, env, assets, slippage_tolerance)
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        self.as_trait().withdraw_liquidity(deps, env, asset)
    }

    fn swap(
        &self,
        deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
    ) -> Result<Response, CwDexError> {
        self.as_trait().swap(deps, env, offer_asset, ask_asset_info, minimum_out_amount)
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        self.as_trait().get_pool_liquidity(deps)
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        asset: AssetList,
    ) -> Result<Asset, CwDexError> {
        self.as_trait().simulate_provide_liquidity(deps, env, asset)
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        self.as_trait().simulate_withdraw_liquidity(deps, asset)
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        self.as_trait().simulate_swap(deps, offer_asset, ask_asset_info, sender)
    }
}
