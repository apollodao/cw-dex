use cosmwasm_std::{Empty, Response};
use cw_asset::Asset;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Staking<O = Empty>: Clone + Serialize + DeserializeOwned {
    fn stake(&self, asset: Asset, options: O) -> Result<Response, CwDexError>;
    fn unstake(&self, asset: Asset, options: O) -> Result<Response, CwDexError>;
    fn claim_rewards(&self, options: O) -> Result<Response, CwDexError>;
    // TODO: add pending rewards query?
}
