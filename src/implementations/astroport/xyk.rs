use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Env, MessageInfo, Response, StdError, StdResult};
use cosmwasm_std::{Deps, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

use astroport_core::pair::PoolResponse;
use delegate::delegate;

use crate::traits::Pool;
use crate::CwDexError;

use super::base_pool::AstroportBasePool;
use super::helpers::AstroAssetList;

#[cw_serde]
pub struct AstroportXykPool(AstroportBasePool);

impl AstroportXykPool {
    pub fn new(pair_addr: Addr, lp_token_addr: Addr) -> Self {
        Self(AstroportBasePool {
            pair_addr,
            lp_token_addr,
        })
    }
}

impl Pool for AstroportXykPool {
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
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        // Math for LP shares calculation when providing liquidity. Copied from the astroport XYK pool
        // implementation. See https://github.com/astroport-fi/astroport-core/blob/5f166bd008257ff241d4dc75a1de6cbfb8415179/contracts/pair/src/contract.rs#L277-L377
        let astro_assets: AstroAssetList = assets.try_into()?;

        let PoolResponse {
            assets: pools,
            total_share,
        } = self.0.get_pool_liquidity_impl(&deps.querier)?;

        let deposits = [
            astro_assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[0].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            astro_assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[1].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() || deposits[1].is_zero() {
            return Err(StdError::generic_err("Either asset cannot be zero").into());
        };

        // map over pools
        const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
        let share = if total_share.is_zero() {
            // Initial share = collateral amount
            let share = Uint128::new(
                (U256::from(deposits[0].u128()) * U256::from(deposits[1].u128()))
                    .integer_sqrt()
                    .as_u128(),
            );

            if share.lt(&MINIMUM_LIQUIDITY_AMOUNT) {
                return Err(StdError::generic_err(
                    "Share cannot be less than minimum liquidity amount",
                )
                .into());
            }

            share
        } else {
            // Assert slippage tolerance
            // assert_slippage_tolerance(slippage_tolerance, &deposits, &pools)?;

            // min(1, 2)
            // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_1))
            // == deposit_0 * total_share / pool_0
            // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
            // == deposit_1 * total_share / pool_1
            std::cmp::min(
                deposits[0].multiply_ratio(total_share, pools[0].amount),
                deposits[1].multiply_ratio(total_share, pools[1].amount),
            )
        };

        let lp_token = Asset {
            info: AssetInfo::Cw20(self.0.lp_token_addr.clone()),
            amount: share,
        };
        Ok(lp_token)
    }
}
