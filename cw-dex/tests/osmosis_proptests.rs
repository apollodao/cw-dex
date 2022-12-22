use cosmwasm_std::{Coin, Uint128};
use cw_asset::{Asset, AssetInfo};
use cw_dex_test_contract::msg::ExecuteMsg;
use cw_dex_test_helpers::osmosis::{setup_pool_and_test_contract, OsmosisPoolType};
use cw_it::helpers::bank_balance_query;

use osmosis_testing::{Module, OsmosisTestApp, RunnerResult, SigningAccount, Wasm};
use proptest::prelude::*;

const TEST_CONTRACT_WASM_FILE_PATH: &str =
    "../target/wasm32-unknown-unknown/release/osmosis_test_contract.wasm";

pub fn setup_pool_and_contract(
    pool_type: OsmosisPoolType,
    initial_liquidity: Vec<u64>,
) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
    setup_pool_and_test_contract(
        pool_type,
        initial_liquidity,
        1_209_600, // Two weeks in seconds
        1,
        TEST_CONTRACT_WASM_FILE_PATH,
    )
}

fn test_multi_pool_provide_liquidity(
    pool_type: OsmosisPoolType,
    initial_liquidity: Vec<u64>,
    added_liquidity: Vec<u64>,
) {
    let (runner, accs, pool_id, contract_addr) =
        setup_pool_and_contract(pool_type, initial_liquidity.clone()).unwrap();

    // We cannot provide more liquidity than the pool has on Osmosis
    let added_liquidity: Vec<u64> = added_liquidity
        .into_iter()
        .zip(initial_liquidity.into_iter())
        .map(|(a, b)| a.min(b) - 1u64)
        .collect();

    let wasm = Wasm::new(&runner);
    let assets: Vec<Coin> = added_liquidity
        .into_iter()
        .enumerate()
        .map(|(i, amount)| Coin {
            denom: format!("denom{}", i),
            amount: amount.into(),
        })
        .collect();

    let provide_msg = ExecuteMsg::ProvideLiquidity {
        assets: assets.clone().into(),
        min_out: Uint128::one(),
    };
    wasm.execute(&contract_addr, &provide_msg, &assets, &accs[0])
        .unwrap();

    // Query LP token balance
    let lp_token_balance =
        bank_balance_query(&runner, contract_addr, format!("gamm/pool/{}", pool_id)).unwrap();

    assert_ne!(lp_token_balance, Uint128::zero());
}

fn test_multi_pool_swap(
    pool_type: OsmosisPoolType,
    pool_liquidity: Vec<u64>,
    offer: Asset,
    ask: AssetInfo,
) {
    let (runner, accs, _pool_id, contract_addr) =
        setup_pool_and_contract(pool_type, pool_liquidity).unwrap();

    let wasm = Wasm::new(&runner);
    let funds = vec![offer.clone().try_into().unwrap()];

    let swap_msg = ExecuteMsg::Swap {
        offer: offer.clone(),
        ask: ask.clone(),
        min_out: Uint128::one(),
    };
    wasm.execute(&contract_addr, &swap_msg, &funds, &accs[0])
        .unwrap();

    // Query LP token balance
    let offer_balance =
        bank_balance_query(&runner, contract_addr.clone(), offer.info.to_string()).unwrap();
    let ask_balance = bank_balance_query(&runner, contract_addr, ask.to_string()).unwrap();

    assert_eq!(offer_balance, Uint128::zero());
    assert_ne!(ask_balance, Uint128::zero());
}

fn setup_swap_test_params(
    pool_liquidity: Vec<u64>,
    offer_idx: usize,
    offer_amount: u64,
    ask_idx: usize,
    n: usize,
) -> (Vec<u64>, Asset, AssetInfo) {
    // Indices mod n
    let offer_idx = offer_idx % n;
    let ask_idx = ask_idx % n;

    // If offer and ask are the same, increment ask mod n
    let ask_idx = if offer_idx == ask_idx {
        (ask_idx + 1) % n
    } else {
        ask_idx
    };

    // Offer amount cannot be larger than the pool liquidity
    let offer_amount = offer_amount.min(pool_liquidity[offer_idx]);

    // Increment any 0s in the pool liquidity
    let pool_liquidity: Vec<u64> = pool_liquidity
        .into_iter()
        .map(|l| if l == 0 { 1 } else { l })
        .collect();

    // Construct offer and ask
    let ask = AssetInfo::Native(format!("denom{}", ask_idx));
    let offer = Asset {
        info: AssetInfo::Native(format!("denom{}", offer_idx)),
        amount: Uint128::from(offer_amount),
    };

    (pool_liquidity, offer, ask)
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Setting both fork and timeout is redundant since timeout implies
        // fork, but both are shown for clarity.
        cases: 16,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_basic_pool_provide_liquidity(initial_liquidity: [u64; 8], added_liquidity: [u64; 8], n in 2usize..8) {
        let initial_liquidity = initial_liquidity.into_iter().take(n).collect();
        let added_liquidity = added_liquidity.into_iter().take(n).collect();
        test_multi_pool_provide_liquidity(OsmosisPoolType::Basic, initial_liquidity, added_liquidity);
    }

    #[test]
    fn test_stable_swap_pool_provide_liquidity(initial_liquidity: [u64; 8], added_liquidity: [u64; 8], scaling_factors: [u32; 8], n in 2usize..8) {
        let initial_liquidity = initial_liquidity.into_iter().take(n).collect();
        let added_liquidity = added_liquidity.into_iter().take(n).collect();
        let scaling_factors = scaling_factors.into_iter().take(n).map(|f| f as u64).collect();
        test_multi_pool_provide_liquidity(OsmosisPoolType::StableSwap { scaling_factors }, initial_liquidity, added_liquidity);
    }

    #[test]
    fn test_balancer_pool_provide_liquidity(initial_liquidity: [u64; 8], added_liquidity: [u64; 8], pool_weights: [u16; 8], n in 2usize..8) {
        let initial_liquidity = initial_liquidity.into_iter().take(n).collect();
        let added_liquidity = added_liquidity.into_iter().take(n).collect();
        let pool_weights = pool_weights.into_iter().take(n).map(|f| f as u64).collect();
        test_multi_pool_provide_liquidity(OsmosisPoolType::Balancer { pool_weights }, initial_liquidity, added_liquidity);
    }

    // Works for swap_amount as u64. Fails for u128. Should be fine
    #[test]
    fn test_basic_pool_swap(pool_liquidity: [u64; 8], offer_idx in 0usize..7, offer_amount: u64, ask_idx in 0usize..7, n in 2usize..8) {
        let (pool_liquidity, offer, ask) = setup_swap_test_params(pool_liquidity.into_iter().take(n).collect(), offer_idx, offer_amount, ask_idx, n);

        test_multi_pool_swap(OsmosisPoolType::Basic, pool_liquidity, offer, ask);
    }

    #[test]
    fn test_stable_swap_pool_swap(pool_liquidity: [u64; 8], offer_idx in 0usize..7, offer_amount: u64, ask_idx in 0usize..7, n in 2usize..8, scaling_factors: [u32; 8]) {
        let (pool_liquidity, offer, ask) = setup_swap_test_params(pool_liquidity.into_iter().take(n).collect(), offer_idx, offer_amount, ask_idx, n);

        let scaling_factors = scaling_factors.into_iter().take(n).map(|f| f as u64).collect();
        test_multi_pool_swap(OsmosisPoolType::StableSwap { scaling_factors }, pool_liquidity, offer, ask);
    }
}
