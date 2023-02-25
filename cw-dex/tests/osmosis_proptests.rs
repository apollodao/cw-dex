use apollo_cw_asset::{Asset, AssetInfo};
use cosmwasm_std::{Addr, Coin, Uint128};
use cw_dex_test_contract::msg::{ExecuteMsg, OsmosisTestContractInstantiateMsg};
use cw_dex_test_helpers::osmosis::setup_pool_and_test_contract;
use cw_dex_test_helpers::robot::CwDexTestRobot;
use cw_it::helpers::bank_balance_query;
use cw_it::osmosis::{test_pool, OsmosisPoolType, OsmosisTestPool};
use cw_it::osmosis_test_tube::Account;

use cw_it::osmosis_test_tube::{Module, OsmosisTestApp, RunnerResult, SigningAccount, Wasm};
use prop::collection::vec;
use proptest::prelude::*;

const TEST_CONTRACT_WASM_FILE_PATH: &str =
    "../target/wasm32-unknown-unknown/release/osmosis_test_contract.wasm";

const TWO_WEEKS_IN_SECONDS: u64 = 1_209_600;

fn setup_pool_and_contract(
    pool: &OsmosisTestPool,
) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
    setup_pool_and_test_contract(
        pool,
        1,
        Some(TWO_WEEKS_IN_SECONDS),
        None,
        TEST_CONTRACT_WASM_FILE_PATH,
    )
}

fn init_account_with_max_balances(app: &OsmosisTestApp, pool: &OsmosisTestPool) -> SigningAccount {
    let coins = pool
        .liquidity
        .iter()
        .map(|c| Coin::new(u128::MAX, &c.denom))
        .collect::<Vec<_>>();

    app.init_account(&coins).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,
        max_global_rejects: 1,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_provide_liquidity(
        (pool,added_liquidity) in test_pool(None).prop_flat_map(|x| {
             (Just(x.clone()), vec(1..u64::MAX, x.liquidity.len()))
        })) {
        let (runner, accs, pool_id, contract_addr) = setup_pool_and_contract(&pool).unwrap();

        let wasm = Wasm::new(&runner);
        let assets: Vec<Coin> = added_liquidity
            .into_iter()
            .enumerate()
            .map(|(i, amount)| Coin {
                denom: pool.liquidity[i].denom.clone(),
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

    #[test]
    fn test_pool_swap(
        (pool,offer_denom,ask_denom, offer_amount) in test_pool(None).prop_flat_map(|x| {
            let len = x.liquidity.len();
            (Just(x), 0usize..len, 0usize..len)
        })
        .prop_filter("Offer and ask can't be the same asset", |(_x, offer_idx, ask_idx)| {
            offer_idx != ask_idx
        })
        .prop_flat_map(|(x, offer_idx, ask_idx)| {
            let denoms = x.liquidity.iter().map(|c| c.denom.clone()).collect::<Vec<_>>();
            (Just(x.clone()), Just(denoms[offer_idx].clone()), Just(denoms[ask_idx].clone()), 1..x.liquidity[offer_idx].amount.u128())
        }),
    ) {
        let offer = Asset {
            info: AssetInfo::Native(offer_denom),
            amount: Uint128::from(offer_amount),
        };
        let ask = AssetInfo::Native(ask_denom);

        let (runner, accs, _pool_id, contract_addr) = setup_pool_and_contract(&pool).unwrap();

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

    #[test]
    fn superfluid_staking_stake_and_unstake(amount in 1..100_000_000_000_000_000_000u128) {
        let app = OsmosisTestApp::new();
        let pool = OsmosisTestPool::new(
            vec![
                Coin::new(1000000000u128, "uatom"),
                Coin::new(1000000000u128, "uosmo"),
            ],
            OsmosisPoolType::Basic,
        );
        let admin = init_account_with_max_balances(&app, &pool);
        let pool_id = pool.create(&app, &admin);

        let validator = Addr::unchecked(app.get_first_validator_address()?);

        println!("validator_addr: {}", validator);

        let init_msg = OsmosisTestContractInstantiateMsg {
            pool_id,
            lock_duration: Some(TWO_WEEKS_IN_SECONDS),
            lock_id: 1u64,
            superfluid_validator: Some(validator),
        };

        // Whitelist LP token for superfluid staking
        app.add_superfluid_lp_share(&format!("gamm/pool/{}", pool_id));

        // Get LP token balance before
        let lp_balance_before = bank_balance_query(
            &app,
            admin.address(),
            format!("gamm/pool/{}", pool_id),
        ).unwrap();
        println!("LP balance before: {}", lp_balance_before);

        println!("Superfluid staking amount: {}", amount);
        let robot = CwDexTestRobot::osmosis(&app, &admin, &init_msg, TEST_CONTRACT_WASM_FILE_PATH);
        let test_contract_addr = robot.test_contract_addr.clone();

        robot
            .superfluid_stake(&admin, amount.into())
            .assert_lp_balance(admin.address(), lp_balance_before.u128() - amount)
            .superfluid_unlock(&admin, amount.into())
            .increase_time(TWO_WEEKS_IN_SECONDS)
            .assert_lp_balance(test_contract_addr, amount);
    }

}
