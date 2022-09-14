use cosmwasm_std::Deps;
use cosmwasm_std::{Addr, Response};
use cw_asset::{Asset, AssetList};
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

/// Trait to represent an AMM pool.
pub trait Pool: Clone + Serialize + DeserializeOwned {
    /// Provide liquidity to the pool.
    ///
    /// Returns a Response with the necessary messages to provide liquidity to the pool.
    fn provide_liquidity(
        &self,
        deps: Deps,
        assets: AssetList,
        recipient: Addr,
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
        offer: Asset,
        ask: Asset,
        // TODO: slippage tolerance
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
}
