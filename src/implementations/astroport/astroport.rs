use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, Empty, Response, StdResult, WasmMsg};
use cosmwasm_std::{Deps, Event, Uint128};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};

use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport_core::pair::ExecuteMsg as PairExecMsg;

use crate::pool::Pool;
use crate::CwDexError;

pub struct AstroportPool {
    /// Information about assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// The token contract code ID used for the tokens in the pool
    pub token_code_id: u64,
    /// The factory contract address
    pub factory_addr: String,
}

impl Pool for AstroportPool {
    fn provide_liquidity(
        &self,
        _deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let mut astro_assets: Vec<AstroAsset> = vec![];
        for asset_base in assets.into_iter() {
            let info = match &asset_base.info {
                AssetInfoBase::Native(denom) => Ok(AstroAssetInfo::NativeToken {
                    denom: denom.to_string(),
                }),
                AssetInfoBase::Cw20(addr) => Ok(AstroAssetInfo::Token {
                    contract_addr: addr.to_owned(),
                }),
                AssetInfoBase::Cw1155(addr, _) => Ok(AstroAssetInfo::Token {
                    contract_addr: addr.to_owned(),
                }),
                x => Err(CwDexError::InvalidInAsset {
                    a: asset_base.to_owned(),
                }),
            }?;
            let amount = asset_base.amount;
            astro_assets.push(AstroAsset {
                info,
                amount,
            })
        }
        let msg = PairExecMsg::ProvideLiquidity {
            assets: astro_assets.clone(),
            slippage_tolerance,
            auto_stake: Some(false), // Should this be true?
            receiver: Some(recipient.to_string()),
        };
        let liquidity = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
            contract_addr: self.factory_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });
        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("factory_addr", &self.factory_addr)
            .add_attribute("assets", format!("{:?}", astro_assets)) // This one is maybe unnecessary
            .add_attribute("recipient", recipient.to_string());
        Ok(Response::new().add_message(liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        todo!()
    }

    fn swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        todo!()
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        todo!()
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        asset: AssetList,
    ) -> Result<Asset, CwDexError> {
        todo!()
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        todo!()
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        todo!()
    }
}
