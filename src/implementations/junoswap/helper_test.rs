use cosmwasm_std::{ Addr, StdError, Uint128, Decimal };
use cw20_0_10_3::Denom;
use test_case::test_case;
use proptest::prelude::*;

use super::helpers::{
    juno_simulate_provide_liquidity,
    JunoProvideLiquidityInfo,
    JunoAsset,
    JunoAssetInfo,
    JunoAssetList,
};
use wasmswap::msg::InfoResponse;

// Edge borders testing
#[test_case(100,0,100,1 => matches Err(_); "when reserve_a is zero should err")]
#[test_case(10000000,1,0,1 => matches Err(_); "when amount_b is zero should err and it goes to the moon instead")]
#[test_case(0,1,0,1 => Ok( (0,0,0) ); "when amount_a and amount_b zero should work")]
#[test_case(2,1,1,1 => with |i: Result<(u128,u128,u128),StdError> | assert!(i.unwrap().2 > 1u128); "when asset_ratio gt pool_ratio with amount_a gt amount_b")]
#[test_case(1,2,1,2 => Ok( (1,1,1) ); "when pool_ratio greater than asset_ratio")]
fn juno_simulate_provide_liquidity_test(
    amount_a: u128,
    reserve_a: u128,
    amount_b: u128,
    reserve_b: u128
) -> Result<(u128, u128, u128), StdError> {
    let usdc = JunoAsset {
        info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("usdc"))),
        amount: Uint128::from(amount_a),
    };
    let dai = JunoAsset {
        info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("dai"))),
        amount: Uint128::from(amount_b),
    };
    let assets: JunoAssetList = JunoAssetList(vec![usdc.to_owned(), dai.to_owned()]);
    let pool_info: InfoResponse = InfoResponse {
        token1_reserve: Uint128::from(reserve_a),
        token1_denom: usdc.info.0,
        token2_reserve: Uint128::from(reserve_b),
        token2_denom: dai.info.0,
        lp_token_supply: usdc.amount + dai.amount, // should this be reserve_a+reserve_b?
        lp_token_address: "lp_token_address".to_string(),
        owner: Some("owner".to_string()),
        lp_fee_percent: Decimal::new(Uint128::from(0u128)),
        protocol_fee_percent: Decimal::new(Uint128::from(0u128)),
        protocol_fee_recipient: "protocol_fee_recipient_addr".to_string(),
    };

    let result: JunoProvideLiquidityInfo = juno_simulate_provide_liquidity(&assets, pool_info)?;

    Ok((
        result.token1_to_use.amount.u128(),
        result.token2_to_use.amount.u128(),
        result.lp_token_expected_amount.u128(),
    ))
}

// Property testing
// proptest! {
//     #![proptest_config(ProptestConfig {
//         //cases: 99,
//         max_global_rejects: 10000,
//         .. ProptestConfig::default()
//       })]
//     #[test]
//     fn compute_current_amp_test_prop_testing(init_amp in 0..1000u64,init_amp_time in 0..1000u64, next_amp in 0..1000u64, next_amp_time in 0..1000u64, block_time in 0..1000u64) {

//         // Requirements
//         prop_assume!(next_amp > init_amp);
//         prop_assume!(next_amp_time > init_amp_time);
//         prop_assume!(block_time > init_amp_time);

//         // Given
//         let pair_info: PairInfo = PairInfo {
//             asset_infos: [
//                 AssetInfo::Token {
//                     contract_addr: Addr::unchecked("asset0000"),
//                 },
//                 AssetInfo::NativeToken {
//                     denom: "uusd".to_string(),
//                 },
//             ],
//             contract_addr: Addr::unchecked("pair0000"),
//             liquidity_token: Addr::unchecked("liquidity0000"),
//             pair_type: PairType::Xyk {},
//         };

//         let config: Config = Config {
//                     pair_info,
//                     factory_addr: Addr::unchecked("addr"),
//                     block_time_last: 0u64,
//                     price0_cumulative_last: Uint128::new(0),
//                     price1_cumulative_last: Uint128::new(0),
//                     init_amp,
//                     init_amp_time,
//                     next_amp,
//                     next_amp_time,
//                 };

//         // When
//         compute_current_amp(&config, block_time)?;

//         // Then Should not panic
//     }
// }