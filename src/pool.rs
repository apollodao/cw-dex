use cosmwasm_std::{Addr, CustomQuery, Response};
use cosmwasm_std::{CosmosMsg, Deps};
use cw_asset::{Asset, AssetList};
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(
        &self,
        deps: Deps,
        assets: AssetList,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;
    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;
    fn swap(
        &self,
        deps: Deps,
        offer: Asset,
        ask: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError>;

    /// Query functions
    fn get_pool_assets(&self, deps: Deps) -> Result<AssetList, CwDexError>;
    fn simulate_provide_liquidity(&self, deps: Deps, asset: AssetList)
        -> Result<Asset, CwDexError>;
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError>;
}
