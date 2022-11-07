use astroport_core::querier::query_supply;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Env, MessageInfo, QuerierWrapper, QueryRequest, Response,
    StdError, StdResult, WasmMsg, WasmQuery,
};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::Cw20ExecuteMsg;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};

use astroport_core::pair::{
    Cw20HookMsg, ExecuteMsg as PairExecMsg, PoolResponse, QueryMsg, SimulationResponse,
};

use crate::CwDexError;

use super::helpers::{astro_asset_info_to_cw_asset_info, cw_asset_to_astro_asset, AstroAssetList};

#[cw_serde]
pub(crate) struct AstroportBasePool {
    pub pair_addr: Addr,
    pub lp_token_addr: Addr,
}

impl AstroportBasePool {
    pub fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, self.lp_token_addr.to_owned())
    }

    pub fn get_pool_liquidity_impl(&self, querier: &QuerierWrapper) -> StdResult<PoolResponse> {
        querier.query::<PoolResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&QueryMsg::Pool {})?,
        }))
    }

    pub fn provide_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        _info: &MessageInfo,
        assets: AssetList,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let astro_assets: AstroAssetList = assets.clone().try_into()?;

        let msg = PairExecMsg::ProvideLiquidity {
            assets: astro_assets
                .0
                .try_into()
                .map_err(|_| CwDexError::Std(StdError::generic_err("invalid assets for pair")))?,
            slippage_tolerance,
            auto_stake: Some(false),
            receiver: None,
        };
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("assets", format!("{:?}", assets));

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    pub fn withdraw_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = &asset.info {
            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {})?,
                })?,
                funds: vec![],
            });

            let event = Event::new("apollo/cw-dex/withdraw_liquidity")
                .add_attribute("pair_addr", &self.pair_addr)
                .add_attribute("asset", format!("{:?}", asset))
                .add_attribute("token_amount", asset.amount);

            Ok(Response::new().add_message(withdraw_liquidity).add_event(event))
        } else {
            Err(CwDexError::InvalidInAsset {
                a: asset,
            })
        }
    }

    pub fn swap(
        &self,
        _deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
    ) -> Result<Response, CwDexError> {
        // Setting belief price to the minimium acceptable return and max spread to zero simplifies things
        // Astroport will make the best possible swap that returns at least minimum_out_amount
        let belief_price = Some(Decimal::from_ratio(offer_asset.amount, minimum_out_amount));
        let swap_msg = match &offer_asset.info {
            AssetInfo::Native(_) => {
                let asset = cw_asset_to_astro_asset(&offer_asset)?;
                Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: self.pair_addr.to_string(),
                    msg: to_binary(&PairExecMsg::Swap {
                        offer_asset: asset,
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    })?,
                    funds: vec![offer_asset.clone().try_into()?],
                }))
            }
            AssetInfo::Cw20(addr) => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&Cw20HookMsg::Swap {
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    })?,
                })?,
                funds: vec![],
            })),
            _ => Err(CwDexError::InvalidInAsset {
                a: offer_asset.clone(),
            }),
        }?;
        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("ask_asset", format!("{:?}", ask_asset_info))
            .add_attribute("offer_asset", format!("{:?}", offer_asset.info))
            .add_attribute("minimum_out_amount", minimum_out_amount);
        Ok(Response::new().add_message(swap_msg).add_event(event))
    }

    pub fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let resp = self.get_pool_liquidity_impl(&deps.querier)?;
        Ok(AssetList::from(AstroAssetList(resp.assets.to_vec())))
    }

    pub fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        let amount = asset.amount;
        let total_share = self.query_lp_token_supply(&deps.querier)?;
        let mut share_ratio = Decimal::zero();
        if !total_share.is_zero() {
            share_ratio = Decimal::from_ratio(amount, total_share);
        }

        let pools = self.get_pool_liquidity_impl(&deps.querier)?.assets;
        Ok(pools
            .iter()
            .map(|a| Asset {
                info: astro_asset_info_to_cw_asset_info(&a.info),
                amount: a.amount * share_ratio,
            })
            .collect::<Vec<Asset>>()
            .into())
    }

    pub fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        _ask_asset_info: AssetInfo,
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        Ok(deps
            .querier
            .query::<SimulationResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.pair_addr.to_string(),
                msg: to_binary(&QueryMsg::Simulation {
                    offer_asset: cw_asset_to_astro_asset(&offer_asset)?,
                })?,
            }))?
            .return_amount)
    }
}
