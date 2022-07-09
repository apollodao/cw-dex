use cosmwasm_std::{Addr, CosmosMsg};
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::CwDexError;

pub trait Pool {
    fn provide_liquidity(
        &self,
        assets: AssetList,
        sender: Option<Addr>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn withdraw_liquidity(
        &self,
        asset: Asset,
        sender: Option<Addr>,
    ) -> Result<CosmosMsg, CwDexError>;
    fn swap_msg(&self, offer: Asset, ask: Asset, sender: Addr) -> Result<CosmosMsg, CwDexError>;
}
