use astroport_core::U256;
use cosmwasm_std::StdResult;
use cw_storage_plus::Item;
use std::cmp::Ordering;

use cosmwasm_std::{Addr, QuerierWrapper, Uint128};


use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport_core::asset::PairInfo;

/// ## Description
/// Returns self multiplied by b
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

pub(crate) const N_COINS: u8 = 2;
const AMP_PRECISION: u64 = 100;
const ITERATIONS: u8 = 32;

/// ## Description
/// Compute the current pool amplification coefficient (AMP).
/// ## Params
/// * **config** is an object of type [`Config`].
///
/// * **env** is an object of type [`Env`].
///
/// This function is needed to calculate how many LP shares a user should get
/// when providing liquidity but is not publicly exposed in the package. Copied
/// from the astro implementation here: https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1492-L1515
pub(crate) fn compute_current_amp(config: &Config, block_time: u64) -> StdResult<u64> {
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

/// ## Description
/// Return a value using a newly specified precision.
/// ## Params
/// * **value** is an object of type [`Uint128`]. This is the value that will
///   have its precision adjusted.
///
/// * **current_precision** is an object of type [`u8`]. This is the `value`'s
///   current precision
///
/// * **new_precision** is an object of type [`u8`]. This is the new precision
///   to use when returning the `value`.
///
/// Copied from the astro code here:
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1269
pub(crate) fn adjust_precision(
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

/// ## Description
/// Computes stable swap invariant (D)
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// ## Params
/// * **leverage** is the object of type [`u128`].
///
/// * **amount_a** is the object of type [`u128`].
///
/// * **amount_b** is the object of type [`u128`].
pub(crate) fn compute_d(leverage: u64, amount_a: u128, amount_b: u128) -> Option<u128> {
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
            //d = (leverage * sum_x + d_p * n_coins) * d / ((leverage - 1) * d + (n_coins +
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

/// ## Description
/// Calculates step
///
/// * **Equation**:
///
/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) *
/// initial_d + (n_coins + 1) * d_product)
pub fn calculate_step(initial_d: &U256, leverage: u64, sum_x: u128, d_product: &U256) -> Option<U256> {
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
pub(crate) fn query_pair_config(querier: &QuerierWrapper, pair: Addr) -> StdResult<Config> {
    Item::<Config>::new("config").query(querier, pair)
}

/// ## Description
/// This structure describes the main control config of pair stable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub(crate) struct Config {
    /// the type of pair info available in [`PairInfo`]
    pub pair_info: PairInfo,
    /// the factory contract address
    pub factory_addr: Addr,
    /// The last time block
    pub block_time_last: u64,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    pub init_amp: u64,
    pub init_amp_time: u64,
    pub next_amp: u64,
    pub next_amp_time: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use astroport_core::{ asset::{ AssetInfo, PairInfo }, factory::PairType };
    use astroport_core::U256;
    use cosmwasm_std::{ Addr, StdError, Uint128 };
    use test_case::test_case;
    use proptest::prelude::*;

    // Edge borders testing
    #[test_case(1,0,0,5, 2 => Ok(5); "block_time greater than config.next_amp_time")]
    #[test_case(1,2,0,0, 0 => matches Err(_); "should panic when init_amp_time greater than next_amp_time")]
    #[test_case(1,0,2,1, 0 => Ok(2); "init_amp greater than next_amp")]
    #[test_case(1,0,1,2, 0 => Ok(1); "next_amp greater than init_amp")]
    #[test_case(2,2,0,0, 1 => matches Err(_); "should panic when init_amp_time greater than blocktime")]
    fn compute_current_amp_test(
        next_amp_time: u64,
        init_amp_time: u64,
        init_amp: u64,
        next_amp: u64,
        block_time: u64
    ) -> Result<u64, StdError> {
        let pair_info: PairInfo = PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0000"),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked("pair0000"),
            liquidity_token: Addr::unchecked("liquidity0000"),
            pair_type: PairType::Xyk {},
        };
        let config: Config = Config {
            pair_info,
            factory_addr: Addr::unchecked("addr"),
            block_time_last: 0u64,
            price0_cumulative_last: Uint128::new(0),
            price1_cumulative_last: Uint128::new(0),
            init_amp,
            init_amp_time,
            next_amp,
            next_amp_time,
        };
        compute_current_amp(&config, block_time)
    }

    // Property testing
    proptest! {
        #![proptest_config(ProptestConfig {
            //cases: 99, 
            max_global_rejects: 10000, 
            .. ProptestConfig::default()
        })]
        #[test]
        fn compute_current_amp_test_prop_testing(init_amp in 0..1000u64,init_amp_time in 0..1000u64, next_amp in 0..1000u64, next_amp_time in 0..1000u64, block_time in 0..1000u64) {
            
            // Requirements
            prop_assume!(next_amp > init_amp);
            prop_assume!(next_amp_time > init_amp_time);
            prop_assume!(block_time > init_amp_time);

            // Given
            let pair_info: PairInfo = PairInfo {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0000"),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                contract_addr: Addr::unchecked("pair0000"),
                liquidity_token: Addr::unchecked("liquidity0000"),
                pair_type: PairType::Xyk {},
            };


            let config: Config = Config {
                        pair_info,
                        factory_addr: Addr::unchecked("addr"),
                        block_time_last: 0u64,
                        price0_cumulative_last: Uint128::new(0),
                        price1_cumulative_last: Uint128::new(0),
                        init_amp,
                        init_amp_time,
                        next_amp,
                        next_amp_time,
                    };

            // When
            compute_current_amp(&config, block_time)?;

            // Then Should not panic
        }
    }

    // Edge borders testing
    #[test_case(10,8,9 => Ok(Uint128::new(100u128)); "should ok when current precision lower than new precision")]
    #[test_case(10,9,8 => Ok(Uint128::new(1u128)); "should ok when new precision lower than current precision")]
    #[test_case(1,255,255 => Ok(Uint128::new(1u128)); "should ok when current and new precision are equals 255")]
    #[test_case(1,0,0 => Ok(Uint128::new(1u128)); "should ok when current and new precision are equals cero")]
    #[test_case(1,0,255 => panics "attempt to multiply with overflow")]
    #[test_case(1,255,0 => panics "attempt to multiply with overflow")]
    fn adjust_precision_test(
        value: u128,
        current_precision: u8,
        new_precision: u8
    ) -> Result<Uint128, StdError> {
        adjust_precision(Uint128::new(value), current_precision, new_precision)
    }

    // Edge borders testing
    #[test_case(10000,10,10 => Some(20);"if a eq b then d should be the sum of both")]
    #[test_case(10,0,0 => Some(0);"if a is zero and b is zero then d should be 0")]
    #[test_case(1000,10,1 => Some(10); "if a is 1000 and b is 10 then d should be 10")]
    #[test_case(1,0,1000 => None;"if a is zero and b is 1000 then d should be 1000")] // FAIL
    #[test_case(1,1000,0 => None;"if a is 1000 and b is zero then d should be 1000")] // FAIL
    fn compute_d_test(leverage: u64, amount_a: u128, amount_b: u128) -> Option<u128> {
        // Computes stable swap invariant (D)
        // `leverage` is use internally in calculate_step
        // N_COINS=2;ITERATIONS=32
        // given A and n
        compute_d(leverage, amount_a, amount_b)
    }

    // Edge borders testing
    #[test_case(0,u64::MAX,u128::MAX,u128::MAX => Some(U256::from(0u128));"should zero if initial_d is zero")]
    #[test_case(0,u64::MAX,1,0 => None;"should be None because r_val=0")] // This return None as a failure
    fn calculate_step_test(
        initial_d: u128,
        leverage: u64,
        sum_x: u128,
        d_product: u128
    ) -> Option<U256> {
        // d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
        calculate_step(&U256::from(initial_d), leverage, sum_x, &U256::from(d_product))
    }
}