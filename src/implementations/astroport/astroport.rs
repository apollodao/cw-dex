use std::sync::mpsc::RecvError;

use astroport_core::factory::PairType;
use astroport_core::querier::query_supply;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, Querier, QuerierWrapper, Response, StdResult,
    WasmMsg,
};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList, AssetListBase};

use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport_core::pair::{Cw20HookMsg, ExecuteMsg as PairExecMsg};

use crate::pool::Pool;
use crate::CwDexError;

use super::helpers::{cw_asset_info_to_astro_asset_info, cw_asset_to_astro_asset, AstroAssetList};

pub struct AstroportXykPool {
    contract_addr: String,
    lp_token_addr: String,
}

impl AstroportXykPool {
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

    fn swap_native_msg(
        &self,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        belief_price: Decimal,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let asset = cw_asset_to_astro_asset(&offer_asset)?;
        let msg = PairExecMsg::Swap {
            offer_asset: asset,
            ask_asset_info: Some(cw_asset_info_to_astro_asset_info(&ask_asset_info)?),
            belief_price: Some(belief_price),
            max_spread: Some(Decimal::zero()),
            to: Some(recipient.into_string()),
        };
        let swap = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.contract_addr.to_owned(),
            msg: to_binary(&msg)?,
            funds: vec![offer_asset.try_into()?],
        });
        Ok(Response::new().add_message(swap))
    }

    fn swap_cw20_msg(
        &self,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        belief_price: Decimal,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = offer_asset.info {
            let swap = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.contract_addr.to_string(),
                    amount: Uint128::zero(), // Should this be `offer_asset.amount`?
                    msg: Cw20ReceiveMsg {
                        sender: recipient.to_string(),
                        amount: offer_asset.amount,
                        msg: to_binary(&Cw20HookMsg::Swap {
                            ask_asset_info: Some(cw_asset_info_to_astro_asset_info(
                                &ask_asset_info,
                            )?),
                            belief_price: Some(belief_price),
                            max_spread: Some(Decimal::zero()),
                            to: Some(recipient.into_string()),
                        })?,
                    }
                    .into_binary()?,
                })?,
                funds: vec![],
            });
            Ok(Response::new().add_message(swap))
        } else {
            Err(CwDexError::InvalidInAsset {
                a: offer_asset,
            })
        }
    }
}

impl Pool for AstroportXykPool {
    fn provide_liquidity(
        &self,
        _deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let astro_assets: Vec<AstroAsset> = AstroAssetList::try_from(assets)?.into();

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
            .add_attribute("pair_addr", &self.contract_addr)
            .add_attribute("assets", format!("{:?}", astro_assets))
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = &asset.info {
            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.contract_addr.to_string(),
                    amount: asset.amount,
                    msg: Cw20ReceiveMsg {
                        sender: recipient.to_string(),
                        amount: asset.amount,
                        msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {
                            assets: vec![],
                        })?,
                    }
                    .into_binary()?,
                })?,
                funds: vec![],
            });

            let event = Event::new("apollo/cw-dex/withdraw_liquidity")
                .add_attribute("pair_addr", &self.contract_addr)
                .add_attribute("asset", format!("{:?}", asset))
                .add_attribute("token_amount", asset.amount)
                .add_attribute("recipient", recipient.to_string());

            Ok(Response::new().add_message(withdraw_liquidity).add_event(event))
        } else {
            Err(CwDexError::InvalidInAsset {
                a: asset,
            })
        }
    }

    fn swap(
        &self,
        _deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        // Setting belief price to the minimium acceptable return and max spread to zero simplifies things
        // Astroport will make the best possible swap that returns at least minimum_out_amount
        let belief_price = Decimal::from_ratio(minimum_out_amount, 1u128);
        let response = match offer_asset.info {
            AssetInfo::Native(_) => {
                self.swap_native_msg(offer_asset.clone(), ask_asset_info, belief_price, recipient)
            }
            AssetInfo::Cw20(_) => {
                self.swap_cw20_msg(offer_asset.clone(), ask_asset_info, belief_price, recipient)
            }
            _ => Err(CwDexError::InvalidInAsset {
                a: offer_asset.clone(),
            }),
        }?;
        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("offer_asset", format!("{:?}", offer_asset));
        Ok(response)
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
