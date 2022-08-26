use cosmwasm_std::{Deps, Empty, Response};
use cw_asset::Asset;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Staking: Clone + Serialize + DeserializeOwned {
    fn stake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError>;
    fn unstake(&self, deps: Deps, asset: Asset) -> Result<Response, CwDexError>;
    fn claim_rewards(&self) -> Result<Response, CwDexError>;
    // TODO: add pending rewards query?
}
