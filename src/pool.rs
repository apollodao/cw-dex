use cosmwasm_std::{CosmosMsg, Empty};
use cw_asset::Asset;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool<O = Empty, A = Asset>: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(
        &self,
        assets: Vec<A>,
        provide_liquidity_options: O,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        asset: A,
        withdraw_liquidity_optioins: O,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap(&self, offer: A, ask: A, swap_options: O) -> Result<CosmosMsg, CwDexError>;
    fn get_pool_assets(&self) -> Result<Vec<A>, CwDexError>;
}
