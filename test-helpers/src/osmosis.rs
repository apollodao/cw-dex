use apollo_cw_asset::{AssetInfo, AssetList};
use cosmwasm_std::{Coin, Uint128};
use cw_dex_test_contract::msg::OsmosisTestContractInstantiateMsg;
use cw_it::helpers::upload_wasm_file;
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

const MAX_SCALE_FACTOR: u64 = 0x7FFF_FFFF_FFFF_FFFF; // 2^63 - 1
const MAX_POOL_WEIGHT: u64 = 1048575; //2^20 - 1

#[derive(Debug, Clone, PartialEq, Eq, Arbitrary)]
pub enum OsmosisPoolType {
    Basic,
    Balancer {
        #[proptest(strategy = "vec(1..MAX_POOL_WEIGHT, param_0.len())")]
        pool_weights: Vec<u64>,
    },
    StableSwap {
        #[proptest(params = "Vec<u64>")]
        #[proptest(value = "params.clone()")]
        scaling_factors: Vec<u64>,
    },
}

#[derive(Debug, Clone)]
pub struct OsmosisTestPool {
    pub assets: Vec<AssetInfo>,
    pub pool_liquidity: Vec<u64>,
    pub pool_type: OsmosisPoolType,
}

prop_compose! {
    /// Generates a touple of vectors with (pool_liquidity, scaling_factors) of size 2..8
    pub fn pool_params()(pool_params in vec((1..u64::MAX, 1..MAX_SCALE_FACTOR), 2..8).prop_filter("scaling factors must be smaller than liquidity",|v| v.iter().all(|(liq, scale)| scale < liq))) -> (Vec<u64>,Vec<u64>) {
         let (pool_liquidity, scaling_factors): (Vec<u64>,Vec<u64>) = pool_params.into_iter().unzip();
            (pool_liquidity, scaling_factors)
    }
}

prop_compose! {
    /// Generates a random OsmosisPoolType with the given scaling factors
    pub fn pool_type(scaling_factors: Vec<u64>)(pool_type in any_with::<OsmosisPoolType>(scaling_factors)) -> OsmosisPoolType {
        pool_type
    }
}

prop_compose! {
    /// Generates a random OsmosisTestPool with 2..8 assets
    pub fn test_pool()(pool_params in pool_params())(pool_type in pool_type(pool_params.clone().1), pool_liquidity in Just(pool_params.0)) -> OsmosisTestPool {
        let mut assets = vec![];
        for i in 0..pool_liquidity.len() {
            assets.push(AssetInfo::Native(format!("denom{}", i)));
        }
        OsmosisTestPool {
            assets,
            pool_liquidity,
            pool_type,
        }
    }
}

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
    pool_type: &OsmosisPoolType,
    initial_liquidity: &[u64],
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
    let pool_id = create_osmosis_pool(&runner, pool_type, &initial_liquidity, &accs[0]);

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
