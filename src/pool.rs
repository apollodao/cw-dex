use cosmwasm_std::CosmosMsg;
use cw_asset::{Asset, AssetList};

use crate::CwDexError;

pub trait Pool<P, W, S> {
    fn provide_liquidity(
        &self,
        assets: AssetList,
        provide_liquidity_options: Option<P>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        asset: Asset,
        withdraw_liquidity_optioins: Option<W>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap(
        &self,
        offer: Asset,
        ask: Asset,
        swap_options: Option<S>,
    ) -> Result<CosmosMsg, CwDexError>;
}
