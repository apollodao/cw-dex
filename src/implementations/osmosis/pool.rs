//! Pool trait implementation for Osmosis

use std::ops::Deref;
use std::str::FromStr;

use apollo_utils::assets::{
    assert_native_asset_info, assert_native_coin, assert_only_native_coins, merge_assets,
};
use apollo_utils::iterators::IntoElementwise;
use osmosis_std::types::osmosis::gamm::v1beta1::{
    GammQuerier, MsgExitPool, MsgJoinPool, MsgJoinSwapShareAmountOut, MsgSwapExactAmountIn,
    SwapAmountInRoute,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Coin, CosmosMsg, Deps, Env, Event, QuerierWrapper, Response, StdError, StdResult, Uint128,
};
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::traits::Pool;
use crate::CwDexError;

use super::helpers::query_lp_denom;

/// Struct for interacting with Osmosis v1beta1 balancer pools. If `pool_id`
/// maps to another type of pool this will fail.
#[cw_serde]
#[derive(Copy)]
pub struct OsmosisPool {
    /// The pool id of the pool to interact with
    pub pool_id: u64,
}

impl OsmosisPool {
    /// Creates a new `OsmosisPool` instance with the given `pool_id`.
    pub fn new(pool_id: u64) -> Self {
        Self { pool_id }
    }

    /// Simulates a single sided join and returns `Uint128` amount of LP tokens
    /// returned. A single sided join will use all of the provided asset.
    pub fn simulate_single_sided_join(
        &self,
        querier: &QuerierWrapper,
        asset: &Asset,
    ) -> StdResult<Uint128> {
        let querier = GammQuerier::new(querier);
        let share_out_amount = Uint128::from_str(
            &querier
                .calc_join_pool_shares(self.pool_id, vec![assert_native_coin(asset)?.into()])?
                .share_out_amount,
        )?;
        Ok(share_out_amount)
    }

    /// Simulates a liquidity provision with all of the assets of the pool.
    /// Returns `(Uint128, AssetList)` amount of LP tokens returned and the
    /// tokens used to join the pool.
    pub fn simulate_noswap_join(
        &self,
        querier: &QuerierWrapper,
        assets: &AssetList,
    ) -> StdResult<(Uint128, AssetList)> {
        let querier = GammQuerier::new(querier);
        let response = &querier.calc_join_pool_no_swap_shares(
            self.pool_id,
            assert_only_native_coins(assets)?.into_elementwise(),
        )?;
        let lp_tokens_returned = Uint128::from_str(&response.shares_out)?;
        let tokens_used: Vec<Coin> = response
            .tokens_out
            .iter()
            .map(|x| {
                Ok(Coin {
                    denom: x.denom.clone(),
                    amount: Uint128::from_str(&x.amount)?,
                })
            })
            .collect::<StdResult<_>>()?;

        Ok((lp_tokens_returned, AssetList::from(tokens_used)))
    }
}

impl Pool for OsmosisPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let mut assets = assets;

        // Remove all zero amount Coins, merge duplicates and assert that all assets are
        // native.
        let assets = assert_only_native_coins(&merge_assets(assets.purge().deref())?)?;

        let expected_shares = self
            .simulate_provide_liquidity(deps, env, assets.to_owned().into())?
            .amount;

        // Assert slippage tolerance
        if min_out > expected_shares {
            return Err(CwDexError::MinOutNotReceived {
                min_out,
                received: expected_shares,
            });
        }

        let join_pool: CosmosMsg = if assets.len() == 1 {
            MsgJoinSwapShareAmountOut {
                sender: env.contract.address.to_string(),
                pool_id: self.pool_id,
                share_out_amount: expected_shares.to_string(),
                token_in_denom: assets[0].denom.to_string(),
                token_in_max_amount: assets[0].amount.to_string(),
            }
            .into()
        } else {
            MsgJoinPool {
                sender: env.contract.address.to_string(),
                pool_id: self.pool_id,
                share_out_amount: expected_shares.to_string(),
                token_in_maxs: assets.into_elementwise(),
            }
            .into()
        };

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("min_out", min_out)
            .add_attribute("expected_shares", expected_shares);

        Ok(Response::new().add_message(join_pool).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        let exit_msg = MsgExitPool {
            sender: env.contract.address.to_string(),
            pool_id: self.pool_id,
            share_in_amount: asset.amount.to_string(),
            token_out_mins: vec![],
        };

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("shares_in", asset.to_string());

        Ok(Response::new().add_message(exit_msg).add_event(event))
    }

    fn swap(
        &self,
        _deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let offer = assert_native_coin(&offer_asset)?;
        let ask_denom = assert_native_asset_info(&ask_asset_info)?;

        // Min out must be greater than 0 for osmosis.
        let min_out = if min_out == Uint128::zero() {
            Uint128::one()
        } else {
            min_out
        };

        let swap_msg = MsgSwapExactAmountIn {
            sender: env.contract.address.to_string(),
            routes: vec![SwapAmountInRoute {
                pool_id: self.pool_id,
                token_out_denom: ask_denom.clone(),
            }],
            token_in: Some(offer.clone().into()),
            token_out_min_amount: min_out.to_string(),
        };

        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pool_id", self.pool_id.to_string())
            .add_attribute("offer", offer.to_string())
            .add_attribute("ask", ask_denom)
            .add_attribute("token_out_min_amount", min_out);

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
        let lp_denom = query_lp_denom(&deps.querier, self.pool_id)?;

        let shares_out_amount: Uint128;
        if assets.len() == 1 {
            shares_out_amount =
                self.simulate_single_sided_join(&deps.querier, &assets.to_vec()[0])?;
        } else {
            (shares_out_amount, _) = self.simulate_noswap_join(&deps.querier, &assets)?;
        }

        Ok(Asset::new(AssetInfo::native(lp_denom), shares_out_amount))
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError> {
        let querier = GammQuerier::new(&deps.querier);
        let lp_token = assert_native_coin(lp_token)?;

        let lp_denom = query_lp_denom(&deps.querier, self.pool_id)?;

        if lp_denom != lp_token.denom {
            return Err(CwDexError::InvalidLpToken {});
        }

        let tokens_out: Vec<Coin> = querier
            .calc_exit_pool_coins_from_shares(self.pool_id, lp_token.amount.to_string())?
            .tokens_out
            .iter()
            .map(|c| {
                Ok(Coin {
                    denom: c.denom.clone(),
                    amount: Uint128::from_str(&c.amount)?,
                })
            })
            .collect::<StdResult<_>>()?;

        Ok(tokens_out.into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer: Asset,
        ask_asset_info: AssetInfo,
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        let offer: Coin = offer.try_into()?;
        let swap_response = GammQuerier::new(&deps.querier).estimate_swap_exact_amount_in(
            sender.ok_or_else(|| StdError::generic_err("sender is required for osmosis"))?,
            self.pool_id,
            format!("{}{}", offer.amount, offer.denom),
            vec![SwapAmountInRoute {
                pool_id: self.pool_id,
                token_out_denom: assert_native_asset_info(&ask_asset_info)?,
            }],
        )?;
        Uint128::from_str(swap_response.token_out_amount.as_str())
    }
}
