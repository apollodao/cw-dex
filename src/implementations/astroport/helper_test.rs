use astroport_core::{ asset::{ AssetInfo, PairInfo }, factory::PairType };
use astroport_core::U256;
use cosmwasm_std::{ Addr, StdError, Uint128 };
use test_case::test_case;
use proptest::prelude::*;
use super::helpers::{ compute_current_amp, Config, adjust_precision, compute_d, calculate_step };

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
#[test_case(10000,10,10 => Some(2);"if a eq b then d should be the sum of both")]
#[test_case(10,0,0 => Some(0);"if a is zero and b is zero then d should be 0")]
#[test_case(1000,10,1 => Some(10); "if a is 1000 and b is 10 then d should be 10")]
#[test_case(1,0,1000 => Some(1000);"if a is zero and b is 1000 then d should be 1000")]
#[test_case(1,1000,0 => Some(1000);"if a is 1000 and b is zero then d should be 1000")]
fn compute_d_test(leverage: u64, amount_a: u128, amount_b: u128) -> Option<u128> {
    // Computes stable swap invariant (D)
    // `leverage` is use internally in calculate_step
    // N_COINS=2;ITERATIONS=32
    // given A and n
    compute_d(leverage, amount_a, amount_b)
}

// Edge borders testing
#[test_case(0,u64::MAX,u128::MAX,u128::MAX => Some(U256::from(0u128));"should zero if initial_d is zero")]
#[test_case(0,u64::MAX,1,0 => None;"should be None because r_val=0")]
//#[test_case(0,u64::MAX,1,0 => Some(U256::from(0u128)))]
fn calculate_step_test(
    initial_d: u128,
    leverage: u64,
    sum_x: u128,
    d_product: u128
) -> Option<U256> {
    // d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
    calculate_step(&U256::from(initial_d), leverage, sum_x, &U256::from(d_product))
}
