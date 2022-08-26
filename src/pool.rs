use cosmwasm_std::{CosmosMsg, Deps, MessageInfo};
use cosmwasm_std::{CustomQuery, Empty};
use cw_asset::{Asset, AssetList};
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool<Q: CustomQuery, O = Empty>: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(
        &self,
        deps: Deps<Q>,
        assets: AssetList,
        options: O,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        deps: Deps<Q>,
        asset: Asset,
        asset_to_withdraw: Option<Asset>,
        options: O,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap(&self, offer: Asset, ask: Asset, options: O) -> Result<CosmosMsg, CwDexError>;

    /// Query functions
    fn get_pool_assets(&self) -> Result<AssetList, CwDexError>;
    fn simulate_provide_liquidity(
        &self,
        deps: Deps<Q>,
        asset: AssetList,
    ) -> Result<Asset, CwDexError>;
    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps<Q>,
        asset: Asset,
        asset_to_withdraw: Option<Asset>,
    ) -> Result<AssetList, CwDexError>;
}
