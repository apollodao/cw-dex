//! Contains an enum with variants for Pool implementations.
//! For use in serialization.

use crate::error::CwDexError;
use crate::traits::pool::Pool as PoolTrait;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Deps, Env, Response, StdResult, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

#[cfg(feature = "astroport")]
use crate::astroport::AstroportPool;

#[cfg(feature = "osmosis")]
use {crate::implementations::osmosis::OsmosisPool, cw_asset::AssetInfoBase, std::str::FromStr};

#[cfg(feature = "junoswap")]
use crate::junoswap::JunoswapPool;

/// An enum with all known variants that implement the Pool trait.
/// The ideal solution would of course instead be to use a trait object so that
/// the caller can pass in any type that implements the Pool trait, but trait
/// objects require us not to implement the Sized trait, which cw_serde
/// requires.
#[cw_serde]
#[non_exhaustive]
pub enum Pool {
    /// Contains an Osmosis pool implementation
    #[cfg(feature = "osmosis")]
    Osmosis(OsmosisPool),
    /// Contains an Junoswap pool implementation
    #[cfg(feature = "junoswap")]
    Junoswap(JunoswapPool),
    /// Contains an Astroport pool implementation
    #[cfg(feature = "astroport")]
    Astroport(AstroportPool),
}

impl Pool {
    /// Returns a specific `Pool` instance as a boxed generic `Pool` trait
    pub fn as_trait(&self) -> Box<dyn PoolTrait> {
        // This is needed to avoid a warning when compiling with all features
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "osmosis")]
            Pool::Osmosis(x) => Box::new(*x),
            #[cfg(feature = "junoswap")]
            Pool::Junoswap(x) => Box::new(x.clone()),
            #[cfg(feature = "astroport")]
            Pool::Astroport(x) => Box::new(x.clone()),
            _ => {
                panic!("Pool variant not supported");
            }
        }
    }

    /// Returns the matching pool given a LP token.
    ///
    /// Arguments:
    /// - `lp_token`: Said LP token
    pub fn get_pool_for_lp_token(deps: Deps, lp_token: &AssetInfo) -> Result<Self, CwDexError> {
        match lp_token {
            #[cfg(feature = "osmosis")]
            AssetInfoBase::Native(lp_token_denom) => {
                //The only Pool implementation that uses native denoms right now is Osmosis
                if !lp_token_denom.starts_with("gamm/pool/") {
                    return Err(CwDexError::NotLpToken {});
                }

                let pool_id_str = lp_token_denom
                    .strip_prefix("gamm/pool/")
                    .ok_or(CwDexError::NotLpToken {})?;

                let pool_id = u64::from_str(pool_id_str).map_err(|_| CwDexError::NotLpToken {})?;

                Ok(Pool::Osmosis(OsmosisPool::new(pool_id, deps)?))
            }
            _ => Err(CwDexError::NotLpToken {}),
        }
    }
}

// Implement the Pool trait for the Pool enum so we can use all the trait
// methods directly on the enum type.
impl PoolTrait for Pool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        self.as_trait()
            .provide_liquidity(deps, env, assets, min_out)
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
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        self.as_trait()
            .swap(deps, env, offer_asset, ask_asset_info, min_out)
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
        asset: &Asset,
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
        self.as_trait()
            .simulate_swap(deps, offer_asset, ask_asset_info, sender)
    }

    fn lp_token(&self) -> AssetInfo {
        self.as_trait().lp_token()
    }

    fn pool_assets(&self, deps: Deps) -> StdResult<Vec<AssetInfo>> {
        self.as_trait().pool_assets(deps)
    }
}
