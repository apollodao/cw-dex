//! Pool trait implementation for Picasso

use apollo_cw_asset::{Asset, AssetInfo, AssetList};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_binary, Coin, CosmosMsg, Deps, Env, Response, StdResult, Uint128, WasmMsg};

use crate::traits::Pool;
use crate::CwDexError;

use super::msg::ExecuteMsg;

/// Picasso pool state
#[cw_serde]
pub struct PicassoPool {
    /// The pool id of the pool to interact with
    pub pool_id: Uint128,
    /// lp token of pool
    pub lp_token_id: String,
    /// first token
    pub base: String,
    /// second token
    pub quote: String,
}

impl Pool for PicassoPool {
    fn provide_liquidity(
        &self,
        __deps: Deps,
        _env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let assets = assets
            .into_iter()
            .map(|asset| match asset.info.clone() {
                apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidInAsset {
                    a: asset.to_owned(),
                }),
                apollo_cw_asset::AssetInfoBase::Native(denom) => {
                    Ok(Coin::new(asset.amount.into(), denom))
                }
            })
            .flatten()
            .collect();

        let msg = super::msg::ExecuteMsg::AddLiquidity {
            pool_id: self.pool_id,
            assets: assets,
            min_mint_amount: min_out,
            keep_alive: true,
        };
        let msg = WasmMsg::Execute {
            contract_addr: "5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg".to_owned(),
            msg: to_binary(&msg)?,
            funds: <_>::default(),
        };
        Ok(Response::new().add_message(msg))
    }

    fn lp_token(&self) -> AssetInfo {
        AssetInfo::Native(self.lp_token_id.to_string())
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        lp_token: Asset,
    ) -> Result<Response, CwDexError> {
        let msg: ExecuteMsg = ExecuteMsg::RemoveLiquidity {
            pool_id: self.pool_id,
            lp_amount: lp_token.amount,
            min_receive: <_>::default(),
        };
        let msg = WasmMsg::Execute {
            contract_addr: "5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg".to_owned(),
            msg: to_binary(&msg)?,
            funds: <_>::default(),
        };
        Ok(Response::new().add_message(CosmosMsg::Wasm(msg)))
    }

    fn swap(
        &self,
        _deps: Deps,
        _env: &Env,
        offer_asset: Asset,
        _ask_asset_info: AssetInfo,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let denom = match offer_asset.info {
            apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidInAsset {
                a: offer_asset.clone(),
            })?,
            apollo_cw_asset::AssetInfoBase::Native(denom) => denom,
        };
        let out = if denom == self.base {
            self.quote.to_string()
        } else {
            self.base.to_string()
        };
        let msg = ExecuteMsg::Swap {
            pool_id: self.pool_id,
            in_asset: Coin::new(offer_asset.amount.into(), denom),
            min_receive: Coin::new(min_out.into(), out),
            keep_alive: true,
        };
        let msg = WasmMsg::Execute {
            contract_addr: "5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg".to_owned(),
            msg: to_binary(&msg)?,
            funds: <_>::default(),
        };
        Ok(Response::new().add_message(CosmosMsg::Wasm(msg)))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let a = deps
            .querier
            .query_balance(
                "5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg",
                self.quote.to_string(),
            )
            .map(|x| Asset::native(x.denom, x.amount))?;
        let b = deps
            .querier
            .query_balance(
                "5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg",
                self.base.to_string(),
            )
            .map(|x| Asset::native(x.denom, x.amount))?;
        Ok(vec![a, b].into())
    }

    fn simulate_provide_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let assets = assets
            .into_iter()
            .map(|asset| match asset.info.clone() {
                apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidInAsset {
                    a: asset.to_owned(),
                }),
                apollo_cw_asset::AssetInfoBase::Native(denom) => {
                    Ok(Coin::new(asset.amount.into(), denom))
                }
            })
            .flatten()
            .collect();

        let msg = super::msg::QueryMsg::SimulateAddLiquidity {
            pool_id: self.pool_id,
            amounts: assets,
        };
        let result: super::msg::SimulateAddLiquidityResponse = _deps
            .querier
            .query_wasm_smart("5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg", &msg)?;
        Ok(Asset::native(self.lp_token_id.to_string(), result.amount))
    }

    fn simulate_withdraw_liquidity(
        &self,
        _deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError> {
        let lp_token = match lp_token.info.clone() {
            apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidInAsset {
                a: lp_token.to_owned(),
            })?,
            apollo_cw_asset::AssetInfoBase::Native(denom) => {
                Coin::new(lp_token.amount.into(), denom)
            }
        };
        let msg = super::msg::QueryMsg::SimulateRemoveLiquidity {
            pool_id: self.pool_id.clone(),
            lp_amount: lp_token.amount,
            min_amount: <_>::default(),
        };
        let result: super::msg::SimulateRemoveLiquidityResponse = _deps
            .querier
            .query_wasm_smart("5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg", &msg)?;
        let result: Vec<_> = result
            .amounts
            .into_iter()
            .map(|x| Asset::native(x.denom, x.amount))
            .collect();
        Ok(result.into())
    }

    fn simulate_swap(
        &self,
        _deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        let offer_asset = match offer_asset.info.clone() {
            apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidInAsset {
                a: offer_asset.to_owned(),
            })?,
            apollo_cw_asset::AssetInfoBase::Native(denom) => {
                Coin::new(offer_asset.amount.into(), denom)
            }
        };
        let ask_asset_info = match ask_asset_info.clone() {
            apollo_cw_asset::AssetInfoBase::Cw20(_) => Err(CwDexError::InvalidOutAsset {})?,
            apollo_cw_asset::AssetInfoBase::Native(denom) => denom,
        };

        let msg = super::msg::QueryMsg::SpotPrice {
            pool_id: self.pool_id.to_owned(),
            base_asset: offer_asset,
            quote_asset_id: ask_asset_info,
            calculate_with_fees: true,
        };

        let result: super::msg::SwapResponse = _deps
            .querier
            .query_wasm_smart("5w3oyasYQg6vkzwETMqUfvtVM99GQ4Xy8mMdKXMgJZDoRYwg", &msg)?;

        Ok(result.value.amount)
    }
}
