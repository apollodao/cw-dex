//! Contains traits related to various forms of staking

use cosmwasm_std::{Addr, Deps, Env, QuerierWrapper, Response, Uint128};
use cw_asset::AssetList;
use cw_utils::Duration as CwDuration;

use crate::error::CwDexError;

/// Defines an interface for claiming and querying rewards accrued from staking
pub trait Rewards {
    /// Claim the pending rewards from the staking contract.
    ///
    /// Arguments:
    ///
    /// Returns a Response containing the messages to claim the pending rewards.
    fn claim_rewards(&self, deps: Deps, env: &Env) -> Result<Response, CwDexError>;

    //// Query the pending rewards in the staking contract that can be claimed by
    /// `user` by calling `claim_rewards`.
    fn query_pending_rewards(
        &self,
        querier: &QuerierWrapper,
        user: &Addr,
    ) -> Result<AssetList, CwDexError>;
}

/// Trait to abstract interaction with a staking contract or module with an optional lockup time.
pub trait Stake: Rewards {
    /// Stake the given assets.
    ///
    /// Arguments:
    /// - `amount`: the amount of the asset to stake.
    ///
    /// Returns a Response containing the messages to stake the given asset.
    fn stake(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;
}

/// Defines an interface for unstaking
pub trait Unstake {
    /// Unstake the given assets.
    ///
    /// Arguments:
    /// - `amount`: the amount of the staked asset to unstake.
    ///
    /// Returns a Response containing the messages to unstake the given asset.
    fn unstake(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;
}

/// A compound trait containing `Stake`, `Unstake` and `Rewards`
pub trait Staking: Stake + Unstake + Rewards {}

/// Defines an interface for unlocking assets
pub trait Unlock {
    /// Start unlocking `amount` of the locked asset. Depending on the implementation,
    /// some kind of unlocking ID will be returned in an event and you may need to handle
    /// this in a reply.
    fn unlock(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;

    /// Claim the assets after they have become fully unlocked. Depending on
    /// implementation, probably requires a call to `unlock` first.
    fn withdraw_unlocked(
        &self,
        deps: Deps,
        env: &Env,
        amount: Uint128,
    ) -> Result<Response, CwDexError>;
}

/// Defines an interface for interacting with locked staked assets
pub trait LockedStaking: Stake + Unlock + Rewards {
    /// Returns the lockup duration for the staked assets.
    fn get_lockup_duration(&self, deps: Deps) -> Result<CwDuration, CwDexError>;
}

/// Defines an interface for forced unlocking of locked assets§
pub trait ForceUnlock: LockedStaking {
    /// Force unlock a lockup position. This can (at least in the case of Osmosis)
    /// only be called by whitelisted addresses and is used in the case of liquidation.
    ///
    /// Arguments:
    /// `lockup_id`: The ID of the lockup position to force unlock.
    /// `assets`: The assets to unlock. If empty, all assets are unlocked.
    fn force_unlock(
        &self,
        deps: Deps,
        env: &Env,
        lockup_id: Option<u64>,
        amount: Uint128,
    ) -> Result<Response, CwDexError>;
}
