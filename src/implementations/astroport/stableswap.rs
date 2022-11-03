use std::collections::HashMap;

use astroport_core::generator::ExecuteMsg as GeneratorExecuteMsg;
use astroport_core::querier::{query_fee_info, query_supply};
use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Decimal256, Env, MessageInfo, QuerierWrapper,
    QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, WasmMsg, WasmQuery,
};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::Cw20ExecuteMsg;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use delegate::delegate;

use astroport_core::asset::Asset as AstroAsset;
use astroport_core::asset::{
    AssetInfo as AstroAssetInfo, Decimal256Ext, DecimalAsset, PairInfo, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport_core::pair::{
    Cw20HookMsg, ExecuteMsg as PairExecMsg, PoolResponse, QueryMsg, SimulationResponse,
};
use itertools::Itertools;

use crate::traits::{Pool, Staking};
use crate::CwDexError;

use super::base_pool::AstroportBasePool;
use super::helpers::{
    astro_asset_info_to_cw_asset_info, compute_current_amp, compute_d,
    cw_asset_info_to_astro_asset_info, cw_asset_to_astro_asset, AstroAssetList,
};
use super::querier::{query_asset_precision, query_pair_config};

#[cw_serde]
pub struct AstroportStableSwapPool(AstroportBasePool);

impl AstroportStableSwapPool {
    pub fn new(pair_addr: Addr, lp_token_addr: Addr) -> Self {
        Self(AstroportBasePool {
            pair_addr,
            lp_token_addr,
        })
    }
}

impl Pool for AstroportStableSwapPool {
    // Delegate all methods except `simulate_provide_liquidity` to the AstroportBasePool implementations
    delegate!(
        to self.0 {
            fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError>;
            fn simulate_withdraw_liquidity(&self, deps: Deps, asset: Asset) -> Result<AssetList, CwDexError>;
            fn simulate_swap(&self, deps: Deps, offer_asset: Asset, ask_asset_info: AssetInfo, sender: Option<String>) -> StdResult<Uint128>;
            fn provide_liquidity(&self, deps: Deps, env: &Env, info: &MessageInfo, assets: AssetList, slippage_tolerance: Option<Decimal>) -> Result<Response, CwDexError>;
            fn withdraw_liquidity(&self, deps: Deps, env: &Env, asset: Asset) -> Result<Response, CwDexError>;
            fn swap(&self, deps: Deps, env: &Env, offer_asset: Asset, ask_asset_info: AssetInfo, minimum_out_amount: Uint128,) -> Result<Response, CwDexError>;
        }
    );

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let astro_assets: AstroAssetList = assets.to_owned().try_into()?;
        let mut pools = vec![];
        for asset in astro_assets.0.clone() {
            pools.push(AstroStableSwapAsset {
                info: asset.info.clone(),
                amount: asset.info.query_pool(&deps.querier, self.0.pair_addr.to_string())?,
                precision: query_asset_precision(
                    &deps.querier,
                    &Addr::unchecked(self.0.pair_addr.to_string()),
                    asset.info,
                )?,
            })
        }
        let config =
            query_pair_config(&deps.querier, &Addr::unchecked(self.0.pair_addr.to_string()))?;

        if assets.len() > config.pair_info.asset_infos.len() {
            return Err(CwDexError::Std(StdError::generic_err("Invalid number of assets. The Astroport supports at least 2 and at most 5 assets within a stable pool")));
        }

        let pools: HashMap<_, _> = config
            .pair_info
            .query_pools(&deps.querier, &env.contract.address)?
            .into_iter()
            .map(|pool| (pool.info, pool.amount))
            .collect();

        let mut non_zero_flag = false;

        let mut assets_collection = astro_assets
            .clone()
            .0
            .into_iter()
            .map(|asset| {
                // Check that at least one asset is non-zero
                if !asset.amount.is_zero() {
                    non_zero_flag = true;
                }

                // Get appropriate pool
                let pool = pools.get(&asset.info).copied().ok_or_else(|| {
                    StdError::generic_err(format!(
                        "The asset {:?} does not belong to the pair",
                        asset
                    ))
                })?;

                Ok((asset, pool))
            })
            .collect::<Result<Vec<_>, CwDexError>>()?;

        // If some assets are omitted then add them explicitly with 0 deposit
        pools.iter().for_each(|(pool_info, pool_amount)| {
            if !astro_assets.0.iter().any(|asset| asset.info.eq(pool_info)) {
                assets_collection.push((
                    AstroAsset {
                        amount: Uint128::zero(),
                        info: pool_info.clone(),
                    },
                    *pool_amount,
                ));
            }
        });

        if !non_zero_flag {
            return Err(CwDexError::Std(StdError::generic_err("Event of zero transfer")));
        }

        let assets_collection = assets_collection
            .iter()
            .cloned()
            .map(|(asset, pool)| {
                let coin_precision = query_asset_precision(
                    &deps.querier,
                    &Addr::unchecked(self.0.pair_addr.to_string()),
                    asset.to_owned().info,
                )?;
                Ok((
                    asset.to_decimal_asset(coin_precision)?,
                    Decimal256::with_precision(pool, coin_precision)?,
                ))
            })
            .collect::<StdResult<Vec<(DecimalAsset, Decimal256)>>>()?;

        let amp = compute_current_amp(deps, &env, config.to_owned())?;

        // Initial invariant (D)
        let old_balances = assets_collection.iter().map(|(_, pool)| *pool).collect_vec();
        let init_d = compute_d(amp, &old_balances, config.greatest_precision)?;

        // Invariant (D) after deposit added
        let mut new_balances = assets_collection
            .iter()
            .map(|(deposit, pool)| Ok(pool + deposit.amount))
            .collect::<StdResult<Vec<_>>>()?;

        let deposit_d = compute_d(amp, &new_balances, config.greatest_precision)?;

        let pair_info: PairInfo =
            deps.querier.query_wasm_smart(self.0.pair_addr.to_string(), &QueryMsg::Pair {})?;

        let n_coins = pair_info.asset_infos.len() as u8;

        let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
        let share = if total_share.is_zero() {
            let share = deposit_d
                .to_uint128_with_precision(config.greatest_precision)?
                .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
                .map_err(|_| {
                    CwDexError::Std(StdError::generic_err(format!(
                        "Initial liquidity must be more than {}",
                        MINIMUM_LIQUIDITY_AMOUNT
                    )))
                })?;

            // share cannot become zero after minimum liquidity subtraction
            if share.is_zero() {
                return Err(CwDexError::Std(StdError::generic_err(format!(
                    "Initial liquidity must be more than {}",
                    MINIMUM_LIQUIDITY_AMOUNT
                ))));
            }

            share
        } else {
            // Get fee info from the factory
            let fee_info = query_fee_info(
                &deps.querier,
                &config.factory_addr,
                config.pair_info.pair_type.clone(),
            )?;

            // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
            let fee = fee_info
                .total_fee_rate
                .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

            let fee = Decimal256::new(fee.atomics().into());

            for i in 0..n_coins as usize {
                let ideal_balance = deposit_d.checked_multiply_ratio(old_balances[i], init_d)?;
                let difference = if ideal_balance > new_balances[i] {
                    ideal_balance - new_balances[i]
                } else {
                    new_balances[i] - ideal_balance
                };
                // Fee will be charged only during imbalanced provide i.e. if invariant D was changed
                new_balances[i] -= fee.checked_mul(difference)?;
            }

            let after_fee_d = compute_d(amp, &new_balances, config.greatest_precision)?;

            let share = Decimal256::with_precision(total_share, config.greatest_precision)?
                .checked_multiply_ratio(after_fee_d.saturating_sub(init_d), init_d)?
                .to_uint128_with_precision(config.greatest_precision)?;

            if share.is_zero() {
                return Err(CwDexError::Std(StdError::generic_err(
                    "Insufficient amount of liquidity",
                )));
            }

            share
        };

        let lp_token = Asset {
            info: AssetInfoBase::Cw20(Addr::unchecked(self.0.lp_token_addr.to_string())),
            amount: share,
        };
        Ok(lp_token)
    }
}

/// This enum describes a Terra asset (native or CW20).
#[cw_serde]
pub struct AstroStableSwapAsset {
    /// Information about an asset stored in a [`AssetInfo`] struct
    pub info: AstroAssetInfo,
    /// A token amount
    pub amount: Uint128,
    /// decimal precision
    pub precision: u8,
}
