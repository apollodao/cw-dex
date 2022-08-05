use cosmwasm_std::{Empty, Response};
use cw_asset::Asset;

use crate::CwDexError;

// TODO: Make stake options non optional. Will probably always be needed
pub trait Staking<S = Empty, U = Empty, C = Empty> {
    fn stake(&self, amount: Asset, stake_options: Option<S>) -> Result<Response, CwDexError>;
    fn unstake(&self, amount: Asset, unstake_options: Option<U>) -> Result<Response, CwDexError>;
    fn claim_rewards(&self, claim_options: Option<C>) -> Result<Response, CwDexError>;
}
