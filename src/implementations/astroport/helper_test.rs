use astroport_core::{ asset::{ AssetInfo, PairInfo }, factory::PairType };
use cosmwasm_std::{ Addr, StdError, Uint128 };
use test_case::test_case;
use proptest::prelude::*;
use super::helpers::{ compute_current_amp, Config };

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