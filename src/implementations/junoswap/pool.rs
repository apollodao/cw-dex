use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, Env, Event, MessageInfo, QuerierWrapper,
    QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use wasmswap::msg::{
    ExecuteMsg, InfoResponse, QueryMsg, Token1ForToken2PriceResponse, TokenSelect,
};

use crate::{traits::Pool, CwDexError};

use super::helpers::{
    juno_get_lp_token_amount_to_mint, juno_get_token1_amount_required,
    juno_get_token2_amount_required, prepare_funds_and_increase_allowances, JunoAsset,
    JunoAssetInfo, JunoAssetList,
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
    // TODO: Does not work when assets are unbalanced. We also need a function that
    // balances the assets before providing liquidity so we can liquidate multiple rewards
    // and provide liquidity.
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        info: &MessageInfo,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let pool_info = self.query_info(&deps.querier)?;

        // Get the assets in the pool from the assets sent in
        let juno_assets: JunoAssetList = assets.clone().try_into()?;
        let token1 = juno_assets.find(pool_info.token1_denom.into())?;
        let token2 = juno_assets.find(pool_info.token2_denom.into())?;

        // Junoswap requires us to specify how many token1 we want to use and
        // calculates itself how many token2 are needed to use the specified
        // amount of token1. Therefore we send (or approve spend) at least this
        // amount of token2 that Junoswap calculates internally. However,
        // we don't want to send extra, nor approve spend on extra, and we want
        // to use as much of both token1 and token2 as possible, so we must
        // calculate exactly how much of each to send.
        // Therefore, we must first check the ratio of assets in the pool and
        // compare with the ratio of assets that are sent to this function to
        // determine which of the assets to use all of and which to not use all of.
        let pool_ratio =
            Decimal::checked_from_ratio(pool_info.token1_reserve, pool_info.token2_reserve)
                .unwrap_or_default();
        let asset_ratio =
            Decimal::checked_from_ratio(token1.amount, token2.amount).unwrap_or_default();

        let token1_to_use;
        let token2_to_use;

        if pool_ratio < asset_ratio {
            // We have a higher ratio of token 1 than the pool, so if we try to use
            // all of our token1 we will get an error because we don't have enough
            // token2. So we must calculate how much of token1 we should use
            // assuming we want to use all of token2.
            token2_to_use = token2.amount;
            token1_to_use = juno_get_token1_amount_required(
                token2_to_use,
                pool_info.token1_reserve,
                pool_info.token2_reserve,
            )?;
        } else {
            // We have a higher ratio of token 2 than token1, so calculate how much
            // token2 to use (and approve spend for, since we don't want to approve
            // spend on any extra).
            token1_to_use = token1.amount;
            token2_to_use = juno_get_token2_amount_required(
                token2.amount,
                token1.amount,
                pool_info.lp_token_supply,
                pool_info.token2_reserve,
                pool_info.token1_reserve,
            )?;
        }

        // Calculate minimum LPs from slippage tolerance
        let expected_lps = juno_get_lp_token_amount_to_mint(
            token1.amount,
            pool_info.lp_token_supply,
            pool_info.token1_reserve,
        )?;

        // TODO: Is this the behavior of slippage_tolerance that we want? Right now
        // It's a bit unclear what slippage_tolerance is supposed to do. We must
        // define it more clearly in the trait doc comments.
        let min_liquidity = expected_lps
            * Decimal::one().checked_sub(slippage_tolerance.unwrap_or_else(|| Decimal::one()))?;

        // Increase allowance for cw20 tokens and add native tokens to the funds vec.
        let (funds, increase_allowances) =
            prepare_funds_and_increase_allowances(env, info, assets, &self.addr)?;

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds,
            msg: to_binary(&ExecuteMsg::AddLiquidity {
                token1_amount: token1_to_use,
                min_liquidity,
                max_token2: token2_to_use,
                expiration: None,
            })?,
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("type", "junoswap")
            .add_attribute("assets", format!("{:?}", vec![token1, token2]))
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new()
            .add_messages(increase_allowances)
            .add_message(provide_liquidity)
            .add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        asset: Asset,
        recipient: Addr,
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

        let output_token: AssetInfo;
        let input_token;
        if JunoAssetInfo(pool_info.token1_denom.clone()) == offer_asset.info {
            input_token = TokenSelect::Token1;
            output_token = JunoAssetInfo(pool_info.token2_denom).into();
        } else if JunoAssetInfo(pool_info.token2_denom) == offer_asset.info {
            input_token = TokenSelect::Token2;
            output_token = JunoAssetInfo(pool_info.token1_denom).into();
        } else {
            return Err(CwDexError::Std(StdError::generic_err("Offered asset is not in the pool")));
        };
        if output_token != ask_asset_info {
            return Err(CwDexError::Std(StdError::generic_err("Asked asset is not in the pool")));
        }

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

        Ok(amount)
    }
}
