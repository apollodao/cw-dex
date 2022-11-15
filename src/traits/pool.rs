use cosmwasm_std::{Deps, Uint128};
use cosmwasm_std::{Env, Response, StdResult};
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::error::CwDexError;
use crate::slippage_control::SlippageControl;

/// Trait to represent an AMM pool.
pub trait Pool {
    /// Provide liquidity to the pool.
    ///
    /// Returns a Response with the necessary messages to provide liquidity to the pool.
    /// `assets` must only contain the assets in the pool, but the ratio of
    /// amounts does not need to be the same as the pool's ratio.
    ///
    /// TODO: Document how slippage_tolerance works. When will it fail?
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        slippage_control: SlippageControl,
    ) -> Result<Response, CwDexError>;

    /// Withdraw liquidity from the pool.
    ///
    /// Arguments:
    /// - `lp_token`: the LP tokens to withdraw as an [`Asset`]. The `info` field must correspond
    ///       to the LP token of the pool. Else, an error is returned.
    ///
    /// Returns a Response containing the messages to withdraw liquidity from the pool.
    fn withdraw_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        lp_token: Asset,
    ) -> Result<Response, CwDexError>;

    /// Swap assets in the pool.
    ///
    /// Arguments:
    /// - `offer`: the offer asset
    /// - `ask`: the ask asset
    ///
    /// Returns a Response containing the messages to swap assets in the pool.
    fn swap(
        &self,
        deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
    ) -> Result<Response, CwDexError>;

    // === Query functions ===

    /// Returns the current balance of the underlying assets in the pool.
    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError>;

    /// Returns an estimated number of LP tokens that would be minted for the given assets.
    ///
    /// Arguments:
    /// - `assets`: the assets to provide liquidity with.
    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError>;

    /// Returns an estimated number of assets to be returned for withdrawing the given LP tokens.
    ///
    /// Arguments:
    /// - `lp_token`: the LP tokens to withdraw as an [`Asset`]. The `info` field must correspond to the
    ///       LP token of the pool. Else, an error is returned.
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: Asset,
    ) -> Result<AssetList, CwDexError>;

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
}
