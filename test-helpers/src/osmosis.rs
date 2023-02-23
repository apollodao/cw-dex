use apollo_cw_asset::{AssetInfo, AssetList};
use cosmwasm_std::{Coin, Uint128};
use cw_dex_test_contract::msg::OsmosisTestContractInstantiateMsg;
use cw_it::helpers::upload_wasm_file;
use cw_it::osmosis::{OsmosisPoolType, OsmosisTestPool};
use osmosis_std::types::osmosis::gamm::poolmodels::balancer::v1beta1::MsgCreateBalancerPool;
use osmosis_std::types::osmosis::gamm::poolmodels::stableswap::v1beta1::{
    MsgCreateStableswapPool, PoolParams as StableSwapPoolParams,
};
use osmosis_std::types::osmosis::gamm::v1beta1::{PoolAsset, PoolParams};
use osmosis_test_tube::{
    Account, Gamm, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount, Wasm,
};
use prop::collection::vec;
use proptest::prelude::{any_with, prop};
use proptest::prop_compose;
use proptest::strategy::{Just, Strategy};
use proptest_derive::Arbitrary;

/// Create an Osmosis pool with the given initial liquidity.
pub fn create_osmosis_pool<'a, R: Runner<'a>>(
    runner: &'a R,
    pool_type: &OsmosisPoolType,
    initial_liquidity: &[Coin],
    signer: &SigningAccount,
) -> u64 {
    let gamm = Gamm::new(runner);
    match pool_type {
        OsmosisPoolType::Basic => {
            gamm.create_basic_pool(initial_liquidity, signer)
                .unwrap()
                .data
                .pool_id
        }
        OsmosisPoolType::Balancer { pool_weights } => {
            gamm.create_balancer_pool(
                MsgCreateBalancerPool {
                    sender: signer.address(),
                    pool_params: Some(PoolParams {
                        swap_fee: "10000000000000000".to_string(),
                        exit_fee: "10000000000000000".to_string(),
                        smooth_weight_change_params: None,
                    }),
                    pool_assets: initial_liquidity
                        .iter()
                        .zip(pool_weights.iter())
                        .map(|(c, weight)| PoolAsset {
                            token: Some(c.clone().into()),
                            weight: weight.to_string(),
                        })
                        .collect(),
                    future_pool_governor: "".to_string(),
                },
                signer,
            )
            .unwrap()
            .data
            .pool_id
        }
        OsmosisPoolType::StableSwap { scaling_factors } => {
            gamm.create_stable_swap_pool(
                MsgCreateStableswapPool {
                    sender: signer.address(),
                    pool_params: Some(StableSwapPoolParams {
                        swap_fee: "10000000000000000".to_string(),
                        exit_fee: "10000000000000000".to_string(),
                    }),
                    initial_pool_liquidity: initial_liquidity
                        .iter()
                        .map(|c| c.clone().into())
                        .collect(),
                    scaling_factors: scaling_factors.clone(),
                    future_pool_governor: "".to_string(),
                    scaling_factor_controller: "".to_string(),
                },
                signer,
            )
            .unwrap()
            .data
            .pool_id
        }
    }
}

/// Setup a pool and test contract for testing.
pub fn setup_pool_and_test_contract(
    pool: &OsmosisTestPool,
    lock_duration: u64,
    lock_id: u64,
    wasm_file_path: &str,
) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
    let runner = OsmosisTestApp::new();

    let mut initial_balances = pool
        .liquidity
        .iter()
        .map(|c| Coin {
            denom: c.denom.clone(),
            amount: Uint128::MAX,
        })
        .collect::<Vec<_>>();
    initial_balances.push(Coin {
        denom: "uosmo".to_string(),
        amount: Uint128::MAX,
    });

    // Initialize 10 accounts with max balance of each token in the pool
    let accs = runner.init_accounts(&initial_balances, 10).unwrap();

    // Create pool
    let pool_id = pool.create(&runner, &accs[0]);

    // Upload test contract wasm file
    let code_id = upload_wasm_file(&runner, &accs[0], wasm_file_path).unwrap();

    // Instantiate the test contract
    let contract_addr =
        instantiate_test_contract(&runner, code_id, pool_id, lock_id, lock_duration, &accs[0])?;

    Ok((runner, accs, pool_id, contract_addr))
}

pub fn instantiate_test_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pool_id: u64,
    lock_id: u64,
    lock_duration: u64,
    signer: &SigningAccount,
) -> RunnerResult<String> {
    let init_msg = OsmosisTestContractInstantiateMsg {
        pool_id,
        lock_duration,
        lock_id,
    };

    let wasm = Wasm::new(runner);
    Ok(wasm
        .instantiate(code_id, &init_msg, None, None, &[], signer)?
        .data
        .address)
}
