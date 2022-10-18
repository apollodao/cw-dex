use astroport_core::querier::query_supply;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Querier, QuerierWrapper, Response, StdResult, WasmMsg,
};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_asset::{Asset, AssetInfo, AssetList, AssetListBase};

use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport_core::pair::{Cw20HookMsg, ExecuteMsg as PairExecMsg};

use crate::pool::Pool;
use crate::CwDexError;

pub struct AstroportPool {
    contract_addr: String,
    lp_token_addr: String,
}

impl AstroportPool {
    fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, &self.lp_token_addr)
    }

    fn query_asset_supply(
        &self,
        querier: &QuerierWrapper,
        asset_info: &AstroAssetInfo,
    ) -> StdResult<Uint128> {
        asset_info.query_pool(querier, &self.contract_addr)
    }
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
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("factory_addr", &self.factory_addr)
            .add_attribute("assets", format!("{:?}", astro_assets)) // This one is maybe unnecessary
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let astro_asset = cw_asset_to_astro_asset(&asset)?;
        // Withdraw asset from pool
        let hook_msg = Cw20HookMsg::WithdrawLiquidity {
            assets: vec![astro_asset.to_owned()],
        };
        let recv_msg = Cw20ReceiveMsg {
            sender: recipient.to_string(),
            amount: asset.amount,
            msg: to_binary(&hook_msg)?,
        };

        // Calculate amount of LP tokens to send, corresponding to withdrawal amount
        let total_asset_supply = self.query_asset_supply(&deps.querier, &astro_asset.info)?;
        let share_ratio = Decimal::from_ratio(asset.amount, total_asset_supply);
        let total_token_supply = self.query_lp_token_supply(&deps.querier)?;
        let token_amount = share_ratio * total_token_supply;
        // Send to pool (pair)
        let exec_msg = Cw20ExecuteMsg::Send {
            contract: self.contract_addr.to_string(),
            amount: token_amount,
            msg: to_binary(&recv_msg)?,
        };

        // Execute on LP token contract
        let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.lp_token_addr.to_string(),
            msg: to_binary(&exec_msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("pair_addr", &self.contract_addr)
            .add_attribute("asset", format!("{:?}", asset))
            .add_attribute("token_amount", token_amount)
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(withdraw_liquidity).add_event(event))
    }

    fn swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        // @todo check if asset is native or token
        // @todo calculate belief price
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
