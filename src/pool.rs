use cosmwasm_std::{Addr, Decimal, Response};
use cosmwasm_std::{Deps, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

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
    ) -> Result<Uint128, CwDexError>;
}
