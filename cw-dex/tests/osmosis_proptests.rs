use std::ops::Range;

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

/// A struct that is allowed to be const that can turned into an OsmosisTestPool
pub struct ConstTestPool<'a> {
    pub pool_type: OsmosisPoolType,
    pub liquidity: &'a [(&'a str, u128)],
}
impl<'a> ConstTestPool<'a> {
    pub const fn new(liquidity: &'a [(&'a str, u128)], pool_type: OsmosisPoolType) -> Self {
        Self {
            pool_type,
            liquidity,
        }
    }
}
impl<'a> From<ConstTestPool<'a>> for OsmosisTestPool {
    fn from(pool: ConstTestPool) -> Self {
        let liquidity = pool
            .liquidity
            .iter()
            .map(|(denom, amount)| Coin::new(*amount, *denom))
            .collect::<Vec<_>>();

        OsmosisTestPool {
            pool_type: pool.pool_type,
            liquidity,
        }
    }
}

const BASIC_TEST_POOL: ConstTestPool = ConstTestPool::new(
    &[("uatom", 1_000_000_000u128), ("uosmo", 1_000_000_000u128)],
    OsmosisPoolType::Basic,
);

// How many LP tokens are minted on first liquidity provision
const TEN_POW_20: u128 = 100000000000000000000u128;

// Starts from the amount of LP tokens that would be equivalent to 1 uosmo
const STAKING_RANGE: Range<u128> = (TEN_POW_20 / 1_000_000_000u128)..TEN_POW_20;

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

/// Setup a test that will use the CwDexTestRobot
fn setup_robot_test<'a>(
    app: &'a OsmosisTestApp,
    pool: &OsmosisTestPool,
) -> RunnerResult<(CwDexTestRobot<'a>, u64, Vec<SigningAccount>)> {
    // Initialize 10 accounts with max balance of each token in the pool
    let accs = app
        .init_accounts(
            &pool
                .liquidity
                .iter()
                .map(|c| Coin::new(u128::MAX, c.denom.clone()))
                .collect::<Vec<_>>(),
            10,
        )
        .unwrap();
    let admin = &accs[0];

    let pool_id = pool.create(app, admin);
    let validator = Addr::unchecked(app.get_first_validator_address()?);

    // Whitelist LP token for superfluid staking
    app.add_superfluid_lp_share(&format!("gamm/pool/{}", pool_id));

    let init_msg = OsmosisTestContractInstantiateMsg {
        pool_id,
        lock_duration: Some(TWO_WEEKS_IN_SECONDS),
        lock_id: 1u64,
        superfluid_validator: Some(validator),
    };

    let robot = CwDexTestRobot::osmosis(&app, &admin, &init_msg, TEST_CONTRACT_WASM_FILE_PATH);

    Ok((robot, pool_id, accs))
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
    fn superfluid_staking_stake_and_unstake(amount in STAKING_RANGE) {
        let app = OsmosisTestApp::new();
        let pool: OsmosisTestPool = BASIC_TEST_POOL.into();

        let (robot, pool_id, accs) = setup_robot_test(&app, &pool).unwrap();
        let test_contract_addr = robot.test_contract_addr.clone();
        let admin = &accs[0];

        // Get LP token balance before
        let lp_balance_before = bank_balance_query(
            &app,
            admin.address(),
            format!("gamm/pool/{}", pool_id),
        ).unwrap();

        robot
            .superfluid_stake(&admin, amount.into())
            .assert_lp_balance(admin.address(), lp_balance_before.u128() - amount)
            .superfluid_unlock(&admin, amount.into())
            .increase_time(TWO_WEEKS_IN_SECONDS)
            .assert_lp_balance(test_contract_addr, amount);
    }

    #[test]
    fn superfluid_staking_stake_twice((amount,amount2) in STAKING_RANGE.prop_flat_map(|x| (1..x).prop_map(move |y| (x-y,y)))) {
        let app = OsmosisTestApp::new();
        let pool: OsmosisTestPool = BASIC_TEST_POOL.into();

        let (robot, pool_id, accs) = setup_robot_test(&app, &pool).unwrap();
        let test_contract_addr = robot.test_contract_addr.clone();
        let admin = &accs[0];

        // Get LP token balance before
        let lp_balance_before = bank_balance_query(
            &app,
            admin.address(),
            format!("gamm/pool/{}", pool_id),
        ).unwrap();

        robot
            .superfluid_stake(&admin, amount.into())
            .assert_lp_balance(admin.address(), lp_balance_before.u128() - amount)
            .superfluid_stake(&admin, amount2.into())
            .assert_lp_balance(admin.address(), lp_balance_before.u128() - amount - amount2)
            .superfluid_unlock(&admin, amount.into())
            .increase_time(TWO_WEEKS_IN_SECONDS)
            .assert_lp_balance(test_contract_addr, amount);
    }

}
