use cosmwasm_std::CosmosMsg;
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::CwDexError;

pub trait Pool {
    fn provide_liquidity(&self, assets: AssetList) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(&self, asset: Asset) -> Result<CosmosMsg, CwDexError>;

    fn market_order(&self, offer: AssetInfo, ask: AssetInfo) -> Result<CosmosMsg, CwDexError>;
    fn limit_order(&self, offer: Asset, ask: Asset) -> Result<CosmosMsg, CwDexError>;
}
