use std::ops::Deref;
use std::str::FromStr;

use apollo_utils::assets::{
    assert_native_asset_info, assert_native_coin, assert_only_native_coins, merge_assets,
};
use osmosis_std::types::osmosis::gamm::v1beta1::{
    GammQuerier, MsgExitPool, MsgJoinSwapExternAmountIn, MsgSwapExactAmountIn, SwapAmountInRoute,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Coin, CosmosMsg, Decimal, Deps, Env, Event, QuerierWrapper, Response, StdError, StdResult,
    Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use osmo_bindings::OsmosisQuery;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::slippage_control::{Price, SlippageControl};
use crate::traits::Pool;
use crate::utils::vec_into;
use crate::CwDexError;

use super::helpers::{BalancerPoolAssets, SupportedPoolType};

/// Struct for interacting with Osmosis v1beta1 balancer pools. If `pool_id` maps to another type of pool this will fail.
#[cw_serde]
#[derive(Copy)]
pub struct OsmosisPool {
    /// The pool id of the pool to interact with
    pub pool_id: u64,
}

impl OsmosisPool {
    pub fn new(pool_id: u64) -> Self {
        Self {
            pool_id,
        }
    }

    fn pool_asset_count(&self, deps: Deps) -> Result<usize, CwDexError> {
        self.get_pool_liquidity(deps).map(|pool| pool.len())
    }

    fn query_pool(&self, deps: &Deps) -> StdResult<SupportedPoolType> {
        let res = GammQuerier::new(&deps.querier).pool(self.pool_id)?;
        res.pool
            .ok_or_else(|| StdError::NotFound {
                kind: "pool".to_string(),
            })?
            .try_into() // convert `Any` to `Pool`
    }

    /// Get the price of `quote_asset` in `base_asset` at the given pool reserves.
    fn price_for_reserves(
        &self,
        deps: Deps,
        reserves: &AssetList,
        base_asset: &str,
        quote_asset: &str,
    ) -> Result<Price, CwDexError> {
        let pool = self.query_pool(&deps)?;
        match pool {
            SupportedPoolType::Balancer(pool) => {
                let pool_assets: BalancerPoolAssets = pool.pool_assets.try_into()?;
                let base_asset_weight = pool_assets.get_pool_weight(base_asset)?;
                let quote_asset_weight = pool_assets.get_pool_weight(&quote_asset)?;

                if base_asset_weight.is_zero() || quote_asset_weight.is_zero() {
                    return Err("pool is misconfigured, got 0 weight".into());
                }

                let base_asset_reserve = reserves
                    .find(&AssetInfo::native(base_asset))
                    .ok_or_else(|| CwDexError::from("Base asset reserve not found in reserves"))?
                    .amount;
                let quote_asset_reserve = reserves
                    .find(&AssetInfo::native(quote_asset))
                    .ok_or_else(|| CwDexError::from("Quote asset reserve not found in reserves"))?
                    .amount;

                if base_asset_reserve.is_zero() || quote_asset_reserve.is_zero() {
                    return Err("Can't get price for empty pool".into());
                }

                let inv_weight_ratio = Decimal::from_ratio(quote_asset_weight, base_asset_weight);
                let supply_ratio = Decimal::from_ratio(base_asset_reserve, quote_asset_reserve);
                let spot_price = supply_ratio.checked_mul(inv_weight_ratio)?;

                Ok(Price {
                    base_asset: AssetInfo::native(base_asset),
                    quote_asset: AssetInfo::native(quote_asset),
                    price: spot_price,
                })
            }
            _ => Err("Price query only implented for Balancer pools".into()),
        }
    }
}

impl Pool for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        slippage_control: SlippageControl,
    ) -> Result<Response, CwDexError> {
        // Construct osmosis querier
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        // Remove all zero amount Coins, merge duplicates and assert that all assets are native.
        let mut assets = assets;
        let assets = assert_only_native_coins(merge_assets(assets.purge().deref())?)?;

        // Validate that we can provide liquidity with the given assets
        let pool_asset_count = self.pool_asset_count(deps)?;
        if assets.len() != pool_asset_count {
            return Err("Must provide liquidity for all assets in the pool".into());
        }

        // TODO: Provide liquidity double sided.
        // For now we only provide liquidity single sided since the ratio of the underlying tokens
        // needs to be exactly the same as the the pool ratio otherwise the remainder is returned
        // and there are no queries yet
        let (join_msgs, shares_out): (Vec<CosmosMsg>, Vec<Uint128>) = assets
            .clone()
            .into_iter()
            .map(|coin| {
                // TODO: Turn into stargate query
                let shares_out =
                    osmosis_calculate_join_pool_shares(querier, self.pool_id, vec![coin.clone()])?
                        .amount;
                Ok((
                    MsgJoinSwapExternAmountIn {
                        sender: env.contract.address.to_string(),
                        pool_id: self.pool_id,
                        token_in: Some(coin.into()),
                        share_out_min_amount: shares_out.to_string(),
                    }
                    .into(),
                    shares_out,
                ))
            })
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .unzip();

        // Slippage control
        let shares_returned = shares_out.iter().sum();
        let pool_type = self.query_pool(&deps)?;
        match slippage_control {
            SlippageControl::MinOut(min_out) => {
                if shares_returned < min_out {
                    return Err(CwDexError::SlippageControlMinOutFailed {
                        wanted: min_out,
                        got: shares_returned,
                    });
                }
            }
            _ => match pool_type {
                SupportedPoolType::StableSwap(_) => {
                    // We currently have no way to get the price of a stableswap pool at
                    // specific reserves, so for now we simply disallow other slippage controls
                    // than MinOut for stableswap pools.
                    // TODO: PR to Osmosis to add query for this.
                    return Err(
                        "Cannot use slippage control other than MinOut for stableswap pools".into(),
                    );
                }
                SupportedPoolType::Balancer(_) => {
                    // Belief price is not well defined for more than two assets since the
                    // liquidity provision affects the price (ratio) of more than two assets.
                    if assets.len() > 2 {
                        return Err("Cannot use slippage control other than MinOut for pool with more than two assets".into());
                    }

                    let first_asset = &assets.to_vec()[0];
                    let second_asset = &assets.to_vec()[1];

                    // If slippage control is BeliefPrice, we use the quote and base assets
                    // the user provided to calculate the price of the pool.
                    // Otherwise we just use the two assets in the order they were provided.
                    let base_asset = slippage_control
                        .get_belief_price()
                        .map_or(first_asset.to_string(), |p| p.base_asset.to_string());
                    let quote_asset = slippage_control
                        .get_belief_price()
                        .map_or(second_asset.to_string(), |p| p.quote_asset.to_string());

                    let reserves = self.get_pool_liquidity(deps)?;
                    let old_price =
                        self.price_for_reserves(deps, &reserves, &base_asset, &quote_asset)?;
                    let new_price =
                        self.price_for_reserves(deps, &reserves, &base_asset, &quote_asset)?;

                    // Assert slippage control
                    slippage_control.assert(old_price, new_price, shares_returned)?;
                }
            },
        }

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("minimum_shares_out", shares_out.iter().sum::<Uint128>().to_string());

        Ok(Response::new().add_messages(join_msgs).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        let lp_token = assert_native_coin(&asset)?;

        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        // TODO: Query for exit pool amounts?
        let token_out_mins = osmosis_calculate_exit_pool_amounts(querier, self.pool_id, &lp_token)?;

        let exit_msg = MsgExitPool {
            sender: env.contract.address.to_string(),
            pool_id: self.pool_id,
            share_in_amount: lp_token.amount.to_string(),
            token_out_mins: vec_into(token_out_mins),
        };

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("lp_token", lp_token.to_string());

        Ok(Response::new().add_message(exit_msg).add_event(event))
    }

    fn swap(
        &self,
        _deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
    ) -> Result<Response, CwDexError> {
        let offer = assert_native_coin(&offer_asset)?;
        let ask_denom = assert_native_asset_info(&ask_asset_info)?;

        let swap_msg = MsgSwapExactAmountIn {
            sender: env.contract.address.to_string(),
            routes: vec![SwapAmountInRoute {
                pool_id: self.pool_id,
                token_out_denom: ask_denom.clone(),
            }],
            token_in: Some(offer.clone().into()),
            token_out_min_amount: minimum_out_amount.to_string(),
        };

        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("offer", offer.to_string())
            .add_attribute("ask", ask_denom)
            .add_attribute("token_out_min_amount", minimum_out_amount);

        Ok(Response::new().add_message(swap_msg).add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let pool_assets = GammQuerier::new(&deps.querier).total_pool_liquidity(self.pool_id)?;

        let asset_list: AssetList = pool_assets
            .liquidity
            .into_iter()
            .map(|coin| {
                Ok(Asset {
                    info: AssetInfo::Native(coin.denom),
                    amount: Uint128::from_str(&coin.amount)?,
                })
            })
            .collect::<StdResult<Vec<Asset>>>()?
            .into();

        Ok(asset_list)
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());
        Ok(osmosis_calculate_join_pool_shares(
            querier,
            self.pool_id,
            assert_only_native_coins(assets)?,
        )?
        .into())
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());
        let lp_token = assert_native_coin(&asset)?;
        Ok(osmosis_calculate_exit_pool_amounts(querier, self.pool_id, &lp_token)?.into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer: Asset,
        _ask_asset_info: AssetInfo,
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        let offer: Coin = offer.try_into()?;
        let swap_response = GammQuerier::new(&deps.querier).estimate_swap_exact_amount_in(
            sender.ok_or(StdError::generic_err("sender is required for osmosis"))?,
            self.pool_id,
            offer.denom.clone(),
            vec![SwapAmountInRoute {
                pool_id: self.pool_id,
                token_out_denom: offer.denom.clone(),
            }],
        )?;
        Uint128::from_str(swap_response.token_out_amount.as_str())
    }
}
