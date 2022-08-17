use cosmwasm_std::{CosmosMsg, Deps, MessageInfo};
use cosmwasm_std::{CustomQuery, Empty};
use cw_asset::Asset;
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool<Q: CustomQuery, O = Empty, A = Asset>: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(
        &self,
        deps: Deps<Q>,
        info: &MessageInfo,
        assets: Vec<A>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        deps: Deps<Q>,
        info: &MessageInfo,
        asset: A,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap(&self, offer: A, ask: A, swap_options: O) -> Result<CosmosMsg, CwDexError>;
    fn get_pool_assets(&self) -> Result<Vec<A>, CwDexError>;
}
