use astroport_core::querier::query_supply;
use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Env, MessageInfo, Response, StdError, StdResult};
use cosmwasm_std::{Deps, Uint128};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use delegate::delegate;
use std::cmp::Ordering;

use crate::traits::Pool;
use crate::CwDexError;
use astroport_core::asset::Asset as AstroAsset;
use astroport_core::asset::AssetInfo as AstroAssetInfo;

use super::base_pool::AstroportBasePool;
use super::helpers::{compute_current_amp, compute_d, AstroAssetList};
use super::querier::{query_asset_precision, query_pair_config};

pub const N_COINS: u8 = 2;

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

    /// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L338
    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let assets: AstroAssetList = assets.to_owned().try_into()?;
        let config = query_pair_config(&deps.querier, &self.0.pair_addr)?;
        let mut pools: [AstroAsset; 2] =
            config.pair_info.query_pools(&deps.querier, self.0.pair_addr.to_owned())?;
        let deposits: [Uint128; 2] = [
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[0].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[1].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() && deposits[1].is_zero() {
            return Err(CwDexError::InvalidZeroAmount {});
        }

        for (i, pool) in pools.iter_mut().enumerate() {
            // we cannot put a zero amount into an empty pool.
            if deposits[i].is_zero() && pool.amount.is_zero() {
                return Err(CwDexError::InvalidProvideLPsWithSingleToken {});
            }

            // transfer only for non zero amount
            if !deposits[i].is_zero() {
                // If the pool is a token contract, then we need to execute a TransferFrom msg to receive funds
                if let AstroAssetInfo::Token {
                    contract_addr: _,
                    ..
                } = &pool.info
                {
                } else {
                    // If the asset is a native token, the pool balance already increased
                    // To calculate the pool balance properly, we should subtract the user deposit from the recorded pool token amount
                    pool.amount = pool.amount.checked_sub(deposits[i])?;
                }
            }
        }

        let token_precision_0 =
            query_asset_precision(&deps.querier, &self.0.pair_addr, pools[0].info.clone())?;
        let token_precision_1 =
            query_asset_precision(&deps.querier, &self.0.pair_addr, pools[1].info.clone())?;

        let greater_precision = token_precision_0.max(token_precision_1);

        let deposit_amount_0 = adjust_precision(deposits[0], token_precision_0, greater_precision)?;
        let deposit_amount_1 = adjust_precision(deposits[1], token_precision_1, greater_precision)?;

        let total_share = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;
        let share = if total_share.is_zero() {
            let liquidity_token_precision = query_asset_precision(
                &deps.querier,
                &self.0.pair_addr,
                AstroAssetInfo::Token {
                    contract_addr: config.pair_info.liquidity_token.clone(),
                },
            )?;

            // Initial share = collateral amount
            adjust_precision(
                Uint128::new(
                    (U256::from(deposit_amount_0.u128()) * U256::from(deposit_amount_1.u128()))
                        .integer_sqrt()
                        .as_u128(),
                ),
                greater_precision,
                liquidity_token_precision,
            )?
        } else {
            let leverage =
                compute_current_amp(&config, &env)?.checked_mul(u64::from(N_COINS)).unwrap();

            let mut pool_amount_0 =
                adjust_precision(pools[0].amount, token_precision_0, greater_precision)?;
            let mut pool_amount_1 =
                adjust_precision(pools[1].amount, token_precision_1, greater_precision)?;

            let d_before_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            pool_amount_0 = pool_amount_0.checked_add(deposit_amount_0)?;
            pool_amount_1 = pool_amount_1.checked_add(deposit_amount_1)?;

            let d_after_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            // d after adding liquidity may be less than or equal to d before adding liquidity because of rounding
            if d_before_addition_liquidity >= d_after_addition_liquidity {
                return Err(CwDexError::Std(StdError::generic_err(
                    "Insufficient amount of liquidity",
                )));
            }

            total_share.multiply_ratio(
                d_after_addition_liquidity - d_before_addition_liquidity,
                d_before_addition_liquidity,
            )
        };

        if share.is_zero() {
            return Err(CwDexError::Std(StdError::generic_err("Insufficient amount of liquidity")));
        }

        let lp_token = Asset {
            info: AssetInfoBase::Cw20(Addr::unchecked(self.0.lp_token_addr.to_string())),
            amount: share,
        };
        Ok(lp_token)
    }
}

/// ## Description
/// Return a value using a newly specified precision.
/// ## Params
/// * **value** is an object of type [`Uint128`]. This is the value that will have its precision adjusted.
///
/// * **current_precision** is an object of type [`u8`]. This is the `value`'s current precision
///
/// * **new_precision** is an object of type [`u8`]. This is the new precision to use when returning the `value`.
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1269
fn adjust_precision(
    value: Uint128,
    current_precision: u8,
    new_precision: u8,
) -> StdResult<Uint128> {
    Ok(match current_precision.cmp(&new_precision) {
        Ordering::Equal => value,
        Ordering::Less => value
            .checked_mul(Uint128::new(10_u128.pow((new_precision - current_precision) as u32)))?,
        Ordering::Greater => value
            .checked_div(Uint128::new(10_u128.pow((current_precision - new_precision) as u32)))?,
    })
}
