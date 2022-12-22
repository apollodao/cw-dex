use cosmwasm_std::{Coin, Uint128};
use cw_asset::AssetList;
use cw_it::helpers::upload_wasm_file;
use osmosis_std::types::osmosis::gamm::poolmodels::balancer::v1beta1::MsgCreateBalancerPool;
use osmosis_std::types::osmosis::gamm::poolmodels::stableswap::v1beta1::{
    MsgCreateStableswapPool, PoolParams as StableSwapPoolParams,
};
use osmosis_std::types::osmosis::gamm::v1beta1::{PoolAsset, PoolParams};
use osmosis_testing::{
    Account, Gamm, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount,
};

use crate::instantiate_test_contract;

pub enum OsmosisPoolType {
    Basic,
    Balancer { pool_weights: Vec<u64> },
    StableSwap { scaling_factors: Vec<u64> },
}

/// Create an Osmosis pool with the given initial liquidity.
pub fn create_osmosis_pool<'a, R: Runner<'a>>(
    runner: &'a R,
    pool_type: OsmosisPoolType,
    initial_liquidity: Vec<Coin>,
    signer: &SigningAccount,
) -> u64 {
    let gamm = Gamm::new(runner);
    match pool_type {
        OsmosisPoolType::Basic => {
            gamm.create_basic_pool(&initial_liquidity, signer)
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
                    scaling_factors,
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
    pool_type: OsmosisPoolType,
    initial_liquidity: Vec<u64>,
    lock_duration: u64,
    lock_id: u64,
    wasm_file_path: &str,
) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
    let runner = OsmosisTestApp::new();

    let initial_liquidity = initial_liquidity
        .iter()
        .enumerate()
        .map(|(i, amount)| Coin {
            denom: format!("denom{}", i),
            amount: (*amount).into(),
        })
        .collect::<Vec<_>>();

    let mut initial_balances = initial_liquidity
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
    let pool_id = create_osmosis_pool(&runner, pool_type, initial_liquidity, &accs[0]);

    // Upload test contract wasm file
    let code_id = upload_wasm_file(&runner, &accs[0], wasm_file_path).unwrap();

    // Instantiate the test contract
    let contract_addr =
        instantiate_test_contract(&runner, code_id, pool_id, lock_id, lock_duration, &accs[0])?;

    Ok((runner, accs, pool_id, contract_addr))
}

/// Create an [`AssetList`] from a slice of tuples of asset denominations and
/// amounts.
pub fn native_assetlist_from_slice(assets: &[(&str, Uint128)]) -> AssetList {
    assets
        .iter()
        .map(|(denom, amount)| Coin {
            denom: denom.to_string(),
            amount: *amount,
        })
        .collect::<Vec<_>>()
        .into()
}
