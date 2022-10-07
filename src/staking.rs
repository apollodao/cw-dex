use cosmwasm_std::{Addr, Deps, Response};
use cw_asset::{Asset, AssetList};
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
}

pub trait Lockup: Clone + Serialize + DeserializeOwned {
    /// Force unlock a lockup position. This can (at least in the case of Osmosis)
    /// only be called by whitelisted addresses and is used in the case of liquidation.
    ///
    /// Arguments:
    /// `lockup_id`: The ID of the lockup position to force unlock.
    /// `assets`: The assets to unlock. If empty, all assets are unlocked.
    fn force_unlock(
        &self,
        deps: Deps,
        lockup_id: u64,
        assets: AssetList,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;

    // ====== Query functions ======

    // TODO: There are probably some other useful queries that can be added here?

    /// Returns the lockup duration for the staked assets.
    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError>;
}
