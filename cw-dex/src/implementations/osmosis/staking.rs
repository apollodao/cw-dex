//! Staking/rewards traits implementations for Osmosis

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Coin, Deps, Env, Event, QuerierWrapper, ReplyOn, Response, StdError, StdResult, SubMsg,
    Uint128,
};
use cw_asset::AssetList;
use cw_utils::Duration as CwDuration;
use osmosis_std::types::osmosis::lockup::{MsgBeginUnlocking, MsgForceUnlock, MsgLockTokens};
use osmosis_std::types::osmosis::superfluid::{
    MsgLockAndSuperfluidDelegate, MsgSuperfluidUnbondLock,
};
use std::time::Duration;

use crate::traits::{ForceUnlock, LockedStaking, Rewards, Stake, Unlock};
use crate::CwDexError;

use super::helpers::ToProtobufDuration;

/// Implementation of locked staking on osmosis. Using the Staking trait.
/// `lockup_duration` is the duration of the lockup period in nano seconds.
#[cw_serde]
pub struct OsmosisStaking {
    /// Lockup duration in nano seconds. Allowed values 1 day, 1 week or 2
    /// weeks.
    pub lockup_duration: Duration,
    /// ID for the lockup record
    pub lock_id: Option<u64>,
    /// Denomination of the associated LP token
    pub lp_token_denom: String,
}

impl OsmosisStaking {
    /// Creates a new OsmosisStaking instance with lock up duration set to
    /// `lockup_duration`.
    ///
    /// Arguments:
    /// - `lockup_duration` is the duration of the lockup period in seconds.
    ///
    /// Returns an error if `lockup_duration` is not one of the allowed values,
    /// 86400, 604800 or 1209600, representing 1 day, 1 week or 2 weeks
    /// respectively.
    pub fn new(
        lockup_duration: u64,
        lock_id: Option<u64>,
        lp_token_denom: String,
    ) -> StdResult<Self> {
        if !(vec![86400u64, 604800u64, 1209600u64].contains(&lockup_duration)) {
            return Err(StdError::generic_err(
                "osmosis error: invalid lockup duration",
            ));
        }
        Ok(Self {
            lockup_duration: Duration::from_secs(lockup_duration),
            lock_id,
            lp_token_denom,
        })
    }
}

/// Reply ID for locking tokens
pub const OSMOSIS_LOCK_TOKENS_REPLY_ID: u64 = 123;
/// Reply ID for unlocking tokens
pub const OSMOSIS_UNLOCK_TOKENS_REPLY_ID: u64 = 124;

impl Rewards for OsmosisStaking {
    fn claim_rewards(&self, _deps: Deps, _env: &Env) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        let event =
            Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "osmosis_staking");
        Ok(Response::new().add_event(event))
    }

    fn query_pending_rewards(
        &self,
        _querier: &QuerierWrapper,
        _user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        // There is no currently no way to query how many have accumulated since
        // last epoch.
        Ok(AssetList::new())
    }
}

impl Stake for OsmosisStaking {
    fn stake(&self, _deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let asset = Coin::new(amount.u128(), self.lp_token_denom.clone());

        let stake_msg = MsgLockTokens {
            owner: env.contract.address.to_string(),
            duration: Some(self.lockup_duration.to_protobuf_duration()),
            coins: vec![asset.clone().into()],
        };

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute(
                "lockup_duration_secs",
                self.lockup_duration.as_secs().to_string(),
            );

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_LOCK_TOKENS_REPLY_ID,
                msg: stake_msg.into(),
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }
}

impl Unlock for OsmosisStaking {
    fn unlock(&self, _deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let asset = Coin::new(amount.u128(), self.lp_token_denom.clone());

        let id = self
            .lock_id
            .ok_or_else(|| StdError::generic_err("osmosis error: lock id not set"))?;

        let unstake_msg = MsgBeginUnlocking {
            owner: env.contract.address.to_string(),
            id,
            coins: vec![asset.clone().into()],
        };

        let event = Event::new("apollo/cw-dex/unstake")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute(
                "lockup_duration_secs",
                self.lockup_duration.as_secs().to_string(),
            )
            .add_attribute("lock_id", id.to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_UNLOCK_TOKENS_REPLY_ID,
                msg: unstake_msg.into(),
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }

    fn withdraw_unlocked(
        &self,
        _deps: Deps,
        _env: &Env,
        _amount: Uint128,
    ) -> Result<Response, CwDexError> {
        // Osmosis automatically sends the unlocked tokens after the lockup duration
        Ok(Response::new())
    }
}

impl LockedStaking for OsmosisStaking {
    fn get_lockup_duration(&self, _deps: Deps) -> Result<CwDuration, CwDexError> {
        Ok(CwDuration::Time(self.lockup_duration.as_secs()))
    }
}

impl ForceUnlock for OsmosisStaking {
    fn force_unlock(
        &self,
        _deps: Deps,
        env: &Env,
        lockup_id: u64,
        amount: Uint128,
    ) -> Result<Response, CwDexError> {
        let coin_to_unlock = Coin::new(amount.u128(), self.lp_token_denom.clone());

        let force_unlock_msg = MsgForceUnlock {
            owner: env.contract.address.to_string(),
            id: lockup_id,
            coins: vec![coin_to_unlock.into()],
        };

        let event = Event::new("apollo/cw-dex/force-unlock")
            .add_attribute("type", "osmosis_staking")
            .add_attribute("amount", amount)
            .add_attribute("lockup_id", lockup_id.to_string());

        Ok(Response::new()
            .add_message(force_unlock_msg)
            .add_event(event))
    }
}

/// Implementation of superfluid staking for osmosis.
#[cw_serde]
pub struct OsmosisSuperfluidStaking {
    validator_address: Addr,
    lock_id: Option<u64>,
    lp_token_denom: String,
}

const TWO_WEEKS_IN_SECS: u64 = 14 * 24 * 60 * 60;

impl OsmosisSuperfluidStaking {
    /// Creates a new instance of `OsmosisSuperfluidStaking`.
    ///
    /// Arguments:
    /// - `validator_address`: Address of the associated validator
    /// - `lock_id`: ID of the lockup record
    /// - `lp_token_denom`: LP token denomination
    pub fn new(
        validator_address: Addr,
        lock_id: Option<u64>,
        lp_token_denom: String,
    ) -> StdResult<Self> {
        Ok(Self {
            validator_address,
            lock_id,
            lp_token_denom,
        })
    }
}

impl Rewards for OsmosisSuperfluidStaking {
    fn claim_rewards(&self, _deps: Deps, _env: &Env) -> Result<Response, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        let event = Event::new("apollo/cw-dex/claim_rewards")
            .add_attribute("type", "osmosis_superfluid_staking");
        Ok(Response::new().add_event(event))
    }

    fn query_pending_rewards(
        &self,
        _querier: &QuerierWrapper,
        _user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        // Rewards are automatically distributed to stakers every epoch.
        // There is no currently no way to query how many have accumulated since
        // last epoch.
        Ok(AssetList::new())
    }
}

impl Stake for OsmosisSuperfluidStaking {
    fn stake(&self, _deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let asset = Coin::new(amount.u128(), self.lp_token_denom.clone());

        let stake_msg = MsgLockAndSuperfluidDelegate {
            sender: env.contract.address.to_string(),
            coins: vec![asset.clone().into()],
            val_addr: self.validator_address.to_string(),
        };

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "osmosis_superfluid_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute("validator_address", self.validator_address.to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: OSMOSIS_LOCK_TOKENS_REPLY_ID,
                msg: stake_msg.into(),
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }
}

impl Unlock for OsmosisSuperfluidStaking {
    fn unlock(&self, _deps: Deps, env: &Env, _amount: Uint128) -> Result<Response, CwDexError> {
        let lock_id = self
            .lock_id
            .ok_or_else(|| StdError::generic_err("osmosis error: lock id not set"))?;

        let unstake_msg = MsgSuperfluidUnbondLock {
            sender: env.contract.address.to_string(),
            lock_id,
        };

        let event = Event::new("apollo/cw-dex/unstake")
            .add_attribute("type", "osmosis_superfluid_staking")
            .add_attribute("validator_address", self.validator_address.to_string())
            .add_attribute("lock_id", lock_id.to_string());

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }

    fn withdraw_unlocked(
        &self,
        _deps: Deps,
        _env: &Env,
        _amount: Uint128,
    ) -> Result<Response, CwDexError> {
        // Osmosis automatically sends the unlocked tokens after the lockup duration
        Ok(Response::new())
    }
}

impl LockedStaking for OsmosisSuperfluidStaking {
    fn get_lockup_duration(&self, _deps: Deps) -> Result<CwDuration, CwDexError> {
        // Lockup time for superfluid staking is always 14 days.
        Ok(CwDuration::Time(TWO_WEEKS_IN_SECS))
    }
}
