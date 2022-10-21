use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, Event, QuerierWrapper, QueryRequest, Response,
    StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use wasmswap::msg::{
    ExecuteMsg, InfoResponse, QueryMsg, Token1ForToken2PriceResponse, TokenSelect,
};

use crate::{traits::Pool, CwDexError};

use super::helpers::{
    juno_get_lp_token_amount_to_mint, juno_get_token2_amount_required, JunoAsset, JunoAssetInfo,
    JunoAssetList,
};

#[cw_serde]
pub struct JunoswapPool {
    pub addr: Addr,
}

impl JunoswapPool {
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
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Get the assets in the pool from the assets sent in
        let juno_assets: JunoAssetList = assets.try_into()?;
        let token1 = juno_assets.find(pool_info.token1_denom.into())?;
        let token2 = juno_assets.find(pool_info.token2_denom.into())?;

        // Calculate minimum LPs from slippage tolerance
        let expected_lps = juno_get_lp_token_amount_to_mint(
            token1.amount,
            pool_info.lp_token_supply,
            pool_info.token1_reserve,
        )?;

        // TODO: checked mul?
        let min_liquidity =
            expected_lps * Decimal::one().checked_sub(slippage_tolerance.unwrap_or_default())?;

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::AddLiquidity {
                token1_amount: token1.amount,
                min_liquidity,
                max_token2: token2.amount, // TODO: correct?
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("type", "junoswap")
            .add_attribute("assets", format!("{:?}", vec![token1, token2]))
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: cosmwasm_std::Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<cosmwasm_std::Response, crate::CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Calculate min tokens out
        let share_ratio = Decimal::from_ratio(pool_info.lp_token_supply, asset.amount);
        let min_token1 = (share_ratio * pool_info.token1_reserve).checked_sub(Uint128::one())?;
        let min_token2 = (share_ratio * pool_info.token2_reserve).checked_sub(Uint128::one())?;

        let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::RemoveLiquidity {
                amount: asset.amount,
                min_token1,
                min_token2,
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("type", "junoswap")
            .add_attribute("asset", format!("{:?}", asset))
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
        let pool_info = self.query_info(&deps.querier)?;

        let input_token = if JunoAssetInfo(pool_info.token1_denom) == offer_asset.info {
            Ok(TokenSelect::Token1)
        } else if JunoAssetInfo(pool_info.token2_denom) == offer_asset.info {
            Ok(TokenSelect::Token2)
        } else {
            Err(StdError::generic_err("Offered asset is not in the pool"))
        }?;

        let swap = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::Swap {
                input_token,
                input_amount: offer_asset.amount,
                min_output: minimum_out_amount,
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("type", "junoswap")
            .add_attribute("offer_asset", format!("{:?}", offer_asset))
            .add_attribute("ask_asset_info", format!("{:?}", ask_asset_info))
            .add_attribute("minimum_out_amount", minimum_out_amount.to_string())
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(swap).add_event(event))
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
        asset: AssetList,
    ) -> Result<Asset, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Get the assets in the pool from the assets sent in
        let juno_assets: JunoAssetList = asset.try_into()?;
        let token1 = juno_assets.find(pool_info.token1_denom.into())?;
        let token2 = juno_assets.find(pool_info.token2_denom.into())?;

        let expected_lps = juno_get_lp_token_amount_to_mint(
            token1.amount,
            pool_info.lp_token_supply,
            pool_info.token1_reserve,
        )?;

        let token2_amount = juno_get_token2_amount_required(
            token2.amount,
            token1.amount,
            pool_info.lp_token_supply,
            pool_info.token2_reserve,
            pool_info.token1_reserve,
        )?;

        if token2_amount > token2.amount {
            return Err(CwDexError::Std(StdError::generic_err(
                "Not enough token2 to provide liquidity",
            )));
        }

        Ok(Asset {
            info: AssetInfo::Cw20(deps.api.addr_validate(&pool_info.lp_token_address)?),
            amount: expected_lps,
        }
        .into())
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Calculate min tokens out
        let share_ratio = Decimal::from_ratio(pool_info.lp_token_supply, asset.amount);
        let min_token1 = (share_ratio * pool_info.token1_reserve).checked_sub(Uint128::one())?;
        let min_token2 = (share_ratio * pool_info.token2_reserve).checked_sub(Uint128::one())?;

        Ok(JunoAssetList(vec![
            JunoAsset {
                info: pool_info.token1_denom.into(),
                amount: min_token1,
            },
            JunoAsset {
                info: pool_info.token2_denom.into(),
                amount: min_token2,
            },
        ])
        .into())
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
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        let pool_info = self.query_info(&deps.querier)?;

        let token1 = JunoAssetInfo(pool_info.token1_denom);
        let token2 = JunoAssetInfo(pool_info.token2_denom);

        let amount = if token1 == offer_asset.info {
            if token2 != ask_asset_info {
                return Err(StdError::generic_err(format!("Invalid ask asset {}", ask_asset_info)));
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
                return Err(StdError::generic_err(format!("Invalid ask asset {}", ask_asset_info)));
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

        if amount < minimum_out_amount {
            return Err(StdError::generic_err(format!(
                "Return amount is too low. {} < {}",
                amount, minimum_out_amount
            )));
        }

        Ok(amount)
    }
}
