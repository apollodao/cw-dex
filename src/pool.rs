use cosmwasm_std::CustomQuery;
use cosmwasm_std::{CosmosMsg, Deps};
use cw_asset::{Asset, AssetList};
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(&self, deps: Deps, assets: AssetList) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(&self, deps: Deps, asset: Asset) -> Result<CosmosMsg, CwDexError>;
    fn swap(&self, deps: Deps, offer: Asset, ask: Asset) -> Result<CosmosMsg, CwDexError>;

    /// Query functions
    fn get_pool_assets(&self) -> Result<AssetList, CwDexError>;
    fn simulate_provide_liquidity(&self, deps: Deps, asset: AssetList)
        -> Result<Asset, CwDexError>;
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError>;
}
