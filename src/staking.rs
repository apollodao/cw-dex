use cosmwasm_std::{Empty, Response};
use cw_asset::Asset;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Staking<O = Empty, A = Asset>: Clone + Serialize + DeserializeOwned {
    fn stake(&self, asset: A, options: O) -> Result<Response, CwDexError>;
    fn unstake(&self, asset: A, options: O) -> Result<Response, CwDexError>;
    fn claim_rewards(&self, options: O) -> Result<Response, CwDexError>;
}
