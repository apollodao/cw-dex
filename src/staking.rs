use cosmwasm_std::{Deps, Response};
use cw_asset::Asset;
use cw_utils::Duration as CwDuration;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Staking: Clone + Serialize + DeserializeOwned {
    fn stake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError>;
    fn unstake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError>;
    fn claim_rewards(&self) -> Result<Response, CwDexError>;
    fn get_lockup_duration(&self) -> Result<CwDuration, CwDexError>;
}
