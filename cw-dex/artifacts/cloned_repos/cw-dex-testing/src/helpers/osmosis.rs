use cosmwasm_std::Coin;
use osmosis_std::types::osmosis::gamm::{
    poolmodels::{
        balancer::v1beta1::MsgCreateBalancerPool,
        stableswap::v1beta1::{MsgCreateStableswapPool, PoolParams as StableSwapPoolParams},
    },
    v1beta1::PoolAsset,
    v1beta1::PoolParams,
};
use osmosis_testing::{Account, Gamm, Module, Runner, SigningAccount};

pub enum OsmosisPoolType {
    Basic,
    Balancer { pool_weights: Vec<u64> },
    StableSwap { scaling_factors: Vec<u64> },
}

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
