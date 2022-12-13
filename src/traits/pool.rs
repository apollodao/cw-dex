//! Contains the `Pool` trait for abstracting the behavior of a dex pool.

use cosmwasm_std::{Deps, Env, Response, StdResult, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::error::CwDexError;

/// Trait to represent an AMM pool.
pub trait Pool {
    /// Provide liquidity to the pool.
    ///
    /// Returns a Response with the necessary messages to provide liquidity to
    /// the pool. `assets` must only contain the assets in the pool, but the
    /// ratio of amounts does not need to be the same as the pool's ratio.
    ///
    /// All implementations of this trait should try to use as much of the
    /// provided assets as possible, but it may leave some in the contracts
    /// balance if they are not exactly in the same ratio as the pool. All
    /// implementations should return an error if the returned amount of LP
    /// tokens is less than `min_out`.
    ///
    /// Arguments:
    /// - `assets`: the assets to provide liquidity with
    /// - `min_out`: the minimum amount of LP tokens to receive
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError>;

    /// Get the LP token for this pool
    fn lp_token(&self) -> AssetInfo;

    /// Withdraw liquidity from the pool.
    ///
    /// Arguments:
    /// - `lp_token`: the LP tokens to withdraw as an [`Asset`]. The `info`
    ///   field must correspond to the LP token of the pool. Else, an error is
    ///   returned.
    ///
    /// Returns a Response containing the messages to withdraw liquidity from
    /// the pool.
    fn withdraw_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        lp_token: Asset,
    ) -> Result<Response, CwDexError>;

    /// Swap assets in the pool.
    ///
    /// Arguments:
    /// - `offer_asset`: The asset we want to swap.
    /// - `ask_asset`: The asset we want to receive from the swap.
    /// - `min_out`: The minimum amount of `ask_asset` to receive.
    ///
    /// Returns a Response containing the messages to swap assets in the pool.
    fn swap(
        &self,
        deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        min_out: Uint128,
    ) -> Result<Response, CwDexError>;

    // === Query functions ===

    /// Returns the current balance of the underlying assets in the pool.
    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError>;

    /// Returns an estimated number of LP tokens that would be minted for the
    /// given assets.
    ///
    /// Arguments:
    /// - `assets`: the assets to provide liquidity with.
    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError>;

    /// Returns an estimated number of assets to be returned for withdrawing the
    /// given LP tokens.
    ///
    /// Arguments:
    /// - `lp_token`: the LP tokens to withdraw as an [`Asset`]. The `info`
    ///   field must correspond to the LP token of the pool. Else, an error is
    ///   returned.
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError>;

    /// Simulates a swap and returns the estimated amount of the asset asked
    /// for, given the offered asset
    ///
    /// Arguments:
    /// - `offer_asset`: The asset offered in the swap
    /// - `ask_asset_info`: The asset asked for in the swap
    /// - `sender`: Sender address (required for Osmosis)
    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        sender: Option<String>,
    ) -> StdResult<Uint128>;

    /// Returns the assets in the pool as a [`Vec<AssetInfo>`]
    fn pool_assets(&self, deps: Deps) -> StdResult<Vec<AssetInfo>> {
        Ok(self
            .get_pool_liquidity(deps)?
            .into_iter()
            .map(|asset| asset.info.clone())
            .collect())
    }
}
