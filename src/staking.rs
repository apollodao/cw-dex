use cosmwasm_std::{Addr, Deps, Response};
use cw_asset::Asset;
use cw_utils::Duration as CwDuration;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

/// Trait to abstract interaction with a staking contract or module with an optional lockup time.
pub trait Staking: Clone + Serialize + DeserializeOwned {
    /// Stake the given assets.
    ///
    /// Arguments:
    /// - `asset`: the asset to stake.
    /// - `recipient`: the address to receive the staked assets.
    ///
    /// Returns a Response containing the messages to stake the given asset.
    fn stake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError>;

    /// Unstake the given assets.
    ///
    /// Arguments:
    /// - `asset`: the asset to unstake.
    ///
    /// Returns a Response containing the messages to unstake the given asset.
    fn unstake(&self, deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError>;

    /// Claim the pending rewards from the staking contract.
    ///
    /// Arguments:
    /// - `recipient`: the address to receive the claimed rewards.
    ///
    /// Returns a Response containing the messages to claim the pending rewards.
    fn claim_rewards(&self, recipient: Addr) -> Result<Response, CwDexError>;

    // ====== Query functions ======

    /// Returns the lockup duration for the staked assets.
    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError>;
}
