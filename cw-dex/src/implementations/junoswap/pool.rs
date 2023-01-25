//! Pool trait implementation for Junoswap

use std::vec;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, Env, Event, QuerierWrapper, QueryRequest, Response, StdError,
    StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use wasmswap::msg::{
    ExecuteMsg, InfoResponse, QueryMsg, Token1ForToken2PriceResponse, TokenSelect,
};

use crate::traits::Pool;
use crate::CwDexError;

use super::helpers::{
    juno_simulate_provide_liquidity, prepare_funds_and_increase_allowances, JunoAsset,
    JunoAssetInfo, JunoAssetList,
};

/// Represents an AMM pool on Astroport
#[cw_serde]
pub struct JunoswapPool {
    /// Address of the pool contract
    pub addr: Addr,
    /// The LP token for this pool
    pub lp_token: Addr,
}

impl JunoswapPool {
    /// Queries the pool contract for information
    pub fn query_info(&self, querier: &QuerierWrapper) -> StdResult<InfoResponse> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.addr.to_string(),
            msg: to_binary(&QueryMsg::Info {})?,
        }))
    }
}

impl Pool for JunoswapPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Calculate minimum LPs from slippage tolerance
        let provide_liquidity_info =
            juno_simulate_provide_liquidity(&assets.try_into()?, pool_info)?;

        // Check if minimum LPs is met
        let lp_out = provide_liquidity_info.lp_token_expected_amount;
        if min_out > lp_out {
            return Err(CwDexError::MinOutNotReceived {
                min_out,
                received: lp_out,
            });
        }

        // Increase allowance for cw20 tokens and add native tokens to the funds vec.
        let assets_to_use = vec![
            provide_liquidity_info.token1_to_use.clone(),
            provide_liquidity_info.token2_to_use.clone(),
        ]
        .into();

        // Separate the assets to pass in the funds and build messages
        // to increase allowances for cw20 tokens.
        let (funds, increase_allowances) =
            prepare_funds_and_increase_allowances(env, &assets_to_use, &self.addr)?;

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds,
            msg: to_binary(&ExecuteMsg::AddLiquidity {
                token1_amount: provide_liquidity_info.token1_to_use.amount,
                min_liquidity: min_out,
                max_token2: provide_liquidity_info.token2_to_use.amount,
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity").add_attribute("type", "junoswap");

        Ok(Response::new()
            .add_messages(increase_allowances)
            .add_message(provide_liquidity)
            .add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::RemoveLiquidity {
                amount: asset.amount,
                min_token1: Uint128::zero(),
                min_token2: Uint128::zero(),
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("type", "junoswap")
            .add_attribute("asset", format!("{:?}", asset));

        Ok(Response::new()
            .add_message(withdraw_liquidity)
            .add_event(event))
    }

    fn swap(
        &self,
        deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        let output_token: AssetInfo;
        let input_token;
        if JunoAssetInfo(pool_info.token1_denom.clone()) == offer_asset.info {
            input_token = TokenSelect::Token1;
            output_token = JunoAssetInfo(pool_info.token2_denom).into();
        } else if JunoAssetInfo(pool_info.token2_denom) == offer_asset.info {
            input_token = TokenSelect::Token2;
            output_token = JunoAssetInfo(pool_info.token1_denom).into();
        } else {
            return Err(CwDexError::Std(StdError::generic_err(
                "Offered asset is not in the pool",
            )));
        };
        if output_token != ask_asset_info {
            return Err(CwDexError::Std(StdError::generic_err(
                "Asked asset is not in the pool",
            )));
        }

        let input_amount = offer_asset.amount;

        // Add native token to the funds vec and build increase allowance message for
        // cw20 token.
        let (funds, increase_allowances) = prepare_funds_and_increase_allowances(
            env,
            &vec![offer_asset.clone()].into(),
            &self.addr,
        )?;

        let swap = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds,
            msg: to_binary(&ExecuteMsg::Swap {
                input_token,
                input_amount,
                min_output: min_out,
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("type", "junoswap")
            .add_attribute("offer_asset", format!("{:?}", offer_asset))
            .add_attribute("ask_asset_info", format!("{:?}", ask_asset_info))
            .add_attribute("minimum_out_amount", min_out.to_string());

        Ok(Response::new()
            .add_messages(increase_allowances)
            .add_message(swap)
            .add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        Ok(JunoAssetList(vec![
            JunoAsset {
                info: pool_info.token1_denom.into(),
                amount: pool_info.token1_reserve,
            },
            JunoAsset {
                info: pool_info.token2_denom.into(),
                amount: pool_info.token2_reserve,
            },
        ])
        .into())
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;
        let lp_addr = deps.api.addr_validate(&pool_info.lp_token_address)?;

        // Calculate minimum LPs from slippage tolerance
        let provide_liquidity_info =
            juno_simulate_provide_liquidity(&assets.try_into()?, pool_info)?;

        Ok(Asset {
            info: AssetInfo::Cw20(lp_addr),
            amount: provide_liquidity_info.lp_token_expected_amount,
        })
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Calculate tokens out
        let token1_amount = lp_token
            .amount
            .checked_mul(pool_info.token1_reserve)?
            .checked_div(pool_info.lp_token_supply)?;
        let token2_amount = lp_token
            .amount
            .checked_mul(pool_info.token2_reserve)?
            .checked_div(pool_info.lp_token_supply)?;

        Ok(JunoAssetList(vec![
            JunoAsset {
                info: pool_info.token1_denom.into(),
                amount: token1_amount,
            },
            JunoAsset {
                info: pool_info.token2_denom.into(),
                amount: token2_amount,
            },
        ])
        .into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        let pool_info = self.query_info(&deps.querier)?;

        let token1 = JunoAssetInfo(pool_info.token1_denom);
        let token2 = JunoAssetInfo(pool_info.token2_denom);

        let amount = if token1 == offer_asset.info {
            if token2 != ask_asset_info {
                return Err(StdError::generic_err(format!(
                    "Invalid ask asset {}",
                    ask_asset_info
                )));
            }

            Ok(deps
                .querier
                .query::<Token1ForToken2PriceResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: self.addr.to_string(),
                    msg: to_binary(&QueryMsg::Token1ForToken2Price {
                        token1_amount: offer_asset.amount,
                    })?,
                }))?
                .token2_amount)
        } else if token2 == offer_asset.info {
            if token1 != ask_asset_info {
                return Err(StdError::generic_err(format!(
                    "Invalid ask asset {}",
                    ask_asset_info
                )));
            }

            Ok(deps
                .querier
                .query::<Token1ForToken2PriceResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: self.addr.to_string(),
                    msg: to_binary(&QueryMsg::Token2ForToken1Price {
                        token2_amount: offer_asset.amount,
                    })?,
                }))?
                .token2_amount)
        } else {
            Err(StdError::generic_err("Offered asset is not in the pool"))
        }?;

        Ok(amount)
    }

    fn lp_token(&self) -> AssetInfo {
        AssetInfo::Cw20(self.lp_token.clone())
    }
}
