#![allow(missing_docs)]

use super::msg::{Config, FactoryQueryMsg, FeeInfo, FeeInfoResponse, PairType};
use cosmwasm_std::{
    to_binary, Addr, Decimal, Env, QuerierWrapper, QueryRequest, StdResult, Uint128, WasmQuery,
};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw_asset::astroport::AstroAssetInfo;
use cw_storage_plus::Item;
use std::cmp::Ordering;

pub const NATIVE_TOKEN_PRECISION: u8 = 6;
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";

pub use uints::U256;
#[allow(clippy::all)]
mod uints {
    use uint::construct_uint;
    construct_uint! {
        pub struct U256(4);
    }
}

/// ## Description
/// Returns self multiplied by b.
pub fn checked_u8_mul(a: &U256, b: u8) -> Option<U256> {
    let mut result = *a;
    for _ in 1..b {
        result = result.checked_add(*a)?;
    }
    Some(result)
}

//
// ============================================================
// ====== Helper functions for Stableswap implementation ======
// ============================================================
//

pub const N_COINS: u8 = 2;
pub const AMP_PRECISION: u64 = 100;
pub const ITERATIONS: u8 = 32;

/// Compute actual amplification coefficient (A)
pub fn compute_current_amp(config: &Config, env: &Env) -> StdResult<u64> {
    let block_time = env.block.time.seconds();

    if block_time < config.next_amp_time {
        let elapsed_time =
            Uint128::from(block_time).checked_sub(Uint128::from(config.init_amp_time))?;
        let time_range =
            Uint128::from(config.next_amp_time).checked_sub(Uint128::from(config.init_amp_time))?;
        let init_amp = Uint128::from(config.init_amp);
        let next_amp = Uint128::from(config.next_amp);

        if config.next_amp > config.init_amp {
            let amp_range = next_amp - init_amp;
            let res = init_amp + (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.u128() as u64)
        } else {
            let amp_range = init_amp - next_amp;
            let res = init_amp - (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.u128() as u64)
        }
    } else {
        Ok(config.next_amp)
    }
}
pub fn adjust_precision(
    value: Uint128,
    current_precision: u8,
    new_precision: u8,
) -> StdResult<Uint128> {
    Ok(match current_precision.cmp(&new_precision) {
        Ordering::Equal => value,
        Ordering::Less => value.checked_mul(Uint128::new(
            10_u128.pow((new_precision - current_precision) as u32),
        ))?,
        Ordering::Greater => value.checked_div(Uint128::new(
            10_u128.pow((current_precision - new_precision) as u32),
        ))?,
    })
}
/// Compute stable swap invariant (D)
/// Equation:
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
pub fn compute_d(leverage: u64, amount_a: u128, amount_b: u128) -> Option<u128> {
    let amount_a_times_coins =
        checked_u8_mul(&U256::from(amount_a), N_COINS)?.checked_add(U256::one())?;
    let amount_b_times_coins =
        checked_u8_mul(&U256::from(amount_b), N_COINS)?.checked_add(U256::one())?;
    let sum_x = amount_a.checked_add(amount_b)?; // sum(x_i), a.k.a S
    if sum_x == 0 {
        Some(0)
    } else {
        let mut d_previous: U256;
        let mut d: U256 = sum_x.into();

        // Newton's method to approximate D
        for _ in 0..ITERATIONS {
            let mut d_product = d;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_a_times_coins)?;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_b_times_coins)?;
            d_previous = d;
            // d = (leverage * sum_x + d_p * n_coins) * d / ((leverage - 1) * d + (n_coins +
            // 1) * d_p);
            d = calculate_step(&d, leverage, sum_x, &d_product)?;
            // Equality with the precision of 1
            if d == d_previous {
                break;
            }
        }
        u128::try_from(d).ok()
    }
}

/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) *
/// initial_d + (n_coins + 1) * d_product)
pub fn calculate_step(
    initial_d: &U256,
    leverage: u64,
    sum_x: u128,
    d_product: &U256,
) -> Option<U256> {
    let leverage_mul = U256::from(leverage).checked_mul(sum_x.into())? / AMP_PRECISION;
    let d_p_mul = checked_u8_mul(d_product, N_COINS)?;

    let l_val = leverage_mul.checked_add(d_p_mul)?.checked_mul(*initial_d)?;

    let leverage_sub =
        initial_d.checked_mul((leverage.checked_sub(AMP_PRECISION)?).into())? / AMP_PRECISION;
    let n_coins_sum = checked_u8_mul(d_product, N_COINS.checked_add(1)?)?;

    let r_val = leverage_sub.checked_add(n_coins_sum)?;

    l_val.checked_div(r_val)
}

// Astroport StableSwap pair does not return needed Config elements with smart
// query Raw query gets all the necessary elements
pub fn query_pair_config(querier: &QuerierWrapper, pair: Addr) -> StdResult<Config> {
    Item::<Config>::new("config").query(querier, pair)
}

pub fn query_token_precision(
    querier: &QuerierWrapper,
    asset_info: AstroAssetInfo,
) -> StdResult<u8> {
    Ok(match asset_info {
        AstroAssetInfo::NativeToken { denom: _ } => NATIVE_TOKEN_PRECISION,
        AstroAssetInfo::Token { contract_addr } => {
            let res: TokenInfoResponse =
                querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;

            res.decimals
        }
    })
}

pub fn query_supply(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<Uint128> {
    let res: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(res.total_supply)
}

pub fn query_fee_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    pair_type: PairType,
) -> StdResult<FeeInfo> {
    let res: FeeInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::FeeInfo { pair_type })?,
    }))?;

    Ok(FeeInfo {
        fee_address: res.fee_address,
        total_fee_rate: Decimal::from_ratio(Uint128::from(res.total_fee_bps), Uint128::new(10000)),
        maker_fee_rate: Decimal::from_ratio(Uint128::from(res.maker_fee_bps), Uint128::new(10000)),
    })
}
