use cosmwasm_std::CustomQuery;
use cosmwasm_std::{CosmosMsg, Deps, MessageInfo};
use cw_asset::{Asset, AssetList};
use serde::{de::DeserializeOwned, Serialize};

use crate::CwDexError;

pub trait Pool<Q: CustomQuery>: Clone + Serialize + DeserializeOwned {
    fn provide_liquidity(
        &self,
        deps: Deps<Q>,
        info: &MessageInfo,
        assets: AssetList,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        deps: Deps<Q>,
        info: &MessageInfo,
        asset: Asset,
        asset_to_withdraw: Option<Asset>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap(&self, info: &MessageInfo, offer: Asset, ask: Asset) -> Result<CosmosMsg, CwDexError>;
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
