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
    Coin, CosmosMsg, Decimal, Deps, Env, Event, MessageInfo, QuerierWrapper, Response, StdError,
    StdResult, Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use osmo_bindings::OsmosisQuery;

use crate::osmosis::osmosis_math::{
    osmosis_calculate_exit_pool_amounts, osmosis_calculate_join_pool_shares,
};
use crate::traits::Pool;
use crate::utils::vec_into;
use crate::CwDexError;

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
}

impl Pool for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        _info: &MessageInfo,
        assets: AssetList,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let mut assets = assets;

        // Remove all zero amount Coins, merge duplicates and assert that all assets are native.
        let assets = assert_only_native_coins(merge_assets(assets.purge().deref())?)?;

        // Construct osmosis querier
        let querier = QuerierWrapper::<OsmosisQuery>::new(deps.querier.deref());

        let slippage_tolerance =
            Decimal::one() - slippage_tolerance.unwrap_or_else(|| Decimal::one());

        // TODO: Provide liquidity double sided.
        // For now we only provide liquidity single sided since the ratio of the underlying tokens
        // needs to be exactly the same as the the pool ratio otherwise the remainder is returned
        // and there are no queries yet
        let (join_msgs, shares_out): (Vec<CosmosMsg>, Vec<Uint128>) = assets
            .into_iter()
            .map(|coin| {
                // TODO: Turn into stargate query
                let shares_out_min = slippage_tolerance
                    * osmosis_calculate_join_pool_shares(
                        querier,
                        self.pool_id,
                        vec![coin.clone()],
                    )?
                    .amount;
                Ok((
                    MsgJoinSwapExternAmountIn {
                        sender: env.contract.address.to_string(),
                        pool_id: self.pool_id,
                        token_in: Some(coin.into()),
                        share_out_min_amount: shares_out_min.to_string(),
                    }
                    .into(),
                    shares_out_min,
                ))
            })
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .unzip();

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
