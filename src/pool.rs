use cosmwasm_std::{Addr, Decimal, Response, StdResult};
use cosmwasm_std::{Deps, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};
use std::str::FromStr;

use crate::osmosis::OsmosisPool;
use crate::CwDexError;

/// Trait to represent an AMM pool.
pub trait Pool {
    /// Provide liquidity to the pool.
    ///
    /// Returns a Response with the necessary messages to provide liquidity to the pool.
    /// `assets` must only contain the assets in the pool, but the ratio of
    /// amounts does not need to be the same as the pool's ratio.
    fn provide_liquidity(
        &self,
        deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError>;

    /// Withdraw liquidity from the pool.
    ///
    /// Arguments:
    /// - `asset`: the LP tokens to withdraw as an [`Asset`]. The `info` field must correspons
    ///       to the LP token of the pool. Else, an error is returned.
    /// - `recipient`: the address to receive the withdrawn assets.
    ///
    /// Returns a Response containing the messages to withdraw liquidity from the pool.
    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;

    /// Swap assets in the pool.
    ///
    /// Arguments:
    /// - `offer`: the offer asset
    /// - `ask`: the ask asset
    /// - `recipient`: the address to receive the swapped assets.
    ///
    /// Returns a Response containing the messages to swap assets in the pool.
    fn swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;

    // === Query functions ===

    /// Returns the current balance of the underlying assets in the pool.
    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError>;

    /// Returns an estimated number of LP tokens that would be minted for the given assets.
    ///
    /// Arguments:
    /// - `assets`: the assets to provide liquidity with.
    fn simulate_provide_liquidity(&self, deps: Deps, asset: AssetList)
        -> Result<Asset, CwDexError>;

    /// Returns an estimated number of assets to be returned for withdrawing the given LP tokens.
    ///
    /// Arguments:
    /// - `asset`: the LP tokens to withdraw as an [`Asset`]. The `info` field must correspond to the
    ///       LP token of the pool. Else, an error is returned.
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError>;

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        sender: Option<String>,
    ) -> StdResult<Uint128>;
}

impl dyn Pool {
    pub fn get_pool_for_lp_token(
        _deps: Deps,
        lp_token: AssetInfo,
    ) -> Result<Box<dyn Pool>, CwDexError> {
        match lp_token {
            cw_asset::AssetInfoBase::Native(lp_token_denom) => {
                //The only Pool implementation that uses native denoms right now is Osmosis
                if !lp_token_denom.starts_with("gamm/pool/") {
                    return Err(CwDexError::NotLpToken {});
                }

                let pool_id_str =
                    lp_token_denom.strip_prefix("gamm/pool/").ok_or(CwDexError::NotLpToken {})?;

                let pool_id = u64::from_str(pool_id_str).map_err(|_| CwDexError::NotLpToken {})?;

                Ok(Box::new(OsmosisPool {
                    pool_id,
                }))
            }
            _ => Err(CwDexError::NotLpToken {}), //TODO: Support Astroport, Junoswap, etc.
        }
    }
}
