use cosmwasm_std::{Deps, Env, Response, Uint128};
use cw_utils::Duration as CwDuration;

use crate::error::CwDexError;

pub trait Rewards {
    /// Claim the pending rewards from the staking contract.
    ///
    /// Arguments:
    /// - `recipient`: the address to receive the claimed rewards.
    ///
    /// Returns a Response containing the messages to claim the pending rewards.
    fn claim_rewards(&self, deps: Deps, env: &Env) -> Result<Response, CwDexError>;
}

/// Trait to abstract interaction with a staking contract or module with an optional lockup time.
pub trait Staking: Rewards {
    /// Stake the given assets.
    ///
    /// Arguments:
    /// - `amount`: the amount of the asset to stake.
    /// - `recipient`: the address to receive the staked assets.
    ///
    /// Returns a Response containing the messages to stake the given asset.
    fn stake(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;

    /// Unstake the given assets.
    ///
    /// Arguments:
    /// - `amount`: the amount of the staked asset to unstake.
    ///
    /// Returns a Response containing the messages to unstake the given asset.
    fn unstake(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;
}

pub trait LockedStaking: Rewards {
    /// Stake and lock `amount` of the asset.
    fn lock(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError>;

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

    /// Returns the lockup duration for the staked assets.
    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError>;
}

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
