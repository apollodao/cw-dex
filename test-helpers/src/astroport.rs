use apollo_cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use apollo_utils::assets::separate_natives_and_cw20s;
use astroport::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport::factory::PairType;
use astroport::pair::{ExecuteMsg as PairExecuteMsg, StablePoolParams};
use cosmwasm_std::{to_json_binary, Addr, Coin, Decimal, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_dex_test_contract::msg::AstroportContractInstantiateMsg;
use cw_it::astroport::utils::{create_astroport_pair, get_local_contracts, setup_astroport};
use cw_it::cw_multi_test::ContractWrapper;
use cw_it::helpers::upload_wasm_file;
use cw_it::test_tube::{Account, Module, Runner, RunnerResult, SigningAccount, Wasm};
use cw_it::traits::CwItRunner;
use cw_it::{ContractType, TestRunner};
use std::str::FromStr;

#[cfg(feature = "osmosis-test-tube")]
use cw_it::Artifact;

use crate::{common_pcl_params, cw20_mint, instantiate_cw20};

/// Setup a pool and test contract for testing.
#[allow(unused_variables)]
pub fn setup_pool_and_test_contract<'a>(
    runner: &'a TestRunner<'a>,
    pool_type: PairType,
    initial_liquidity: Vec<(&str, u64)>,
    native_denom_count: usize,
    wasm_file_path: &str,
) -> RunnerResult<(Vec<SigningAccount>, String, String, String, AssetList)> {
    let wasm = Wasm::new(runner);

    // Initialize 10 accounts with max balance of each token
    let mut initial_balances = (0..native_denom_count)
        .map(|i| Coin {
            denom: format!("denom{}", i),
            amount: Uint128::MAX,
        })
        .collect::<Vec<_>>();
    initial_balances.push(Coin {
        denom: "uatom".to_string(),
        amount: Uint128::MAX,
    });
    initial_balances.push(Coin {
        denom: "uluna".to_string(),
        amount: Uint128::MAX,
    });
    initial_balances.push(Coin {
        denom: "uosmo".to_string(),
        amount: Uint128::MAX,
    });

    let accs = runner.init_accounts(&initial_balances, 10).unwrap();

    let admin = &accs[0];

    let contracts = get_local_contracts(runner, &None, false, &None);

    // Instantiate Astroport contracts
    let astroport_contracts = setup_astroport(runner, contracts, admin);

    // Update native coin registry with uluna precision
    wasm.execute(
        &astroport_contracts.coin_registry.address,
        &astroport::native_coin_registry::ExecuteMsg::Add {
            native_coins: vec![("uluna".to_string(), 6)],
        },
        &[],
        admin,
    )
    .unwrap();

    // Update native coin registry with uatom precision
    wasm.execute(
        &astroport_contracts.coin_registry.address,
        &astroport::native_coin_registry::ExecuteMsg::Add {
            native_coins: vec![("uatom".to_string(), 6)],
        },
        &[],
        admin,
    )
    .unwrap();

    // Instantiate Apollo token (to have second CW20 to test CW20-CW20 pools)
    let apollo_token = instantiate_cw20(
        runner,
        astroport_contracts.astro_token.code_id,
        &Cw20InstantiateMsg {
            name: "APOLLO".to_string(),
            symbol: "APOLLO".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: admin.address(),
                cap: None,
            }),
            marketing: None,
        },
        admin,
    )
    .unwrap();

    // Mint Cw20 tokens to accounts
    for account in &accs {
        // Mint Astro tokens
        cw20_mint(
            runner,
            astroport_contracts.clone().astro_token.address,
            account.address().clone(),
            Uint128::from(1_000_000_000_000_000_000u128),
            admin,
        )
        .unwrap();
        // Mint Apollo tokens
        cw20_mint(
            runner,
            apollo_token.clone(),
            account.address().clone(),
            Uint128::from(1_000_000_000_000_000_000u128),
            admin,
        )
        .unwrap();
    }

    // Create AssetList for initial liquidity
    let mut asset_list = AssetList::new();
    for (asset, amount) in initial_liquidity.into_iter() {
        if asset == "astro" {
            asset_list
                .add(&Asset::new(
                    AssetInfo::Cw20(Addr::unchecked(
                        astroport_contracts.clone().astro_token.address,
                    )),
                    Uint128::new(amount.into()),
                ))
                .unwrap();
        } else if asset == "uluna" {
            asset_list
                .add(&Asset::new(
                    AssetInfo::Native("uluna".to_string()),
                    Uint128::new(amount.into()),
                ))
                .unwrap();
        } else if asset == "apollo" {
            asset_list
                .add(&Asset::new(
                    AssetInfo::Cw20(Addr::unchecked(apollo_token.clone())),
                    Uint128::new(amount.into()),
                ))
                .unwrap();
        } else if asset == "uatom" {
            asset_list
                .add(&Asset::new(
                    AssetInfo::Native("uatom".to_string()),
                    Uint128::new(amount.into()),
                ))
                .unwrap();
        }
    }

    // Convert AssetList to Astro Assets
    let mut astro_asset_infos = vec![];
    let mut astro_assets = vec![];
    for asset in asset_list.into_iter() {
        match &asset.info {
            AssetInfoBase::Cw20(addr) => {
                let asset_info = AstroAssetInfo::Token {
                    contract_addr: addr.to_owned(),
                };
                astro_asset_infos.push(asset_info.clone());
                astro_assets.push(AstroAsset {
                    info: asset_info,
                    amount: asset.amount,
                })
            }
            AssetInfoBase::Native(denom) => {
                let asset_info = AstroAssetInfo::NativeToken {
                    denom: denom.to_string(),
                };
                astro_asset_infos.push(asset_info.clone());
                astro_assets.push(AstroAsset {
                    info: asset_info,
                    amount: asset.amount,
                })
            }
        }
    }

    // Create pool
    let init_params = match &pool_type {
        PairType::Stable {} => Some(
            to_json_binary(&StablePoolParams {
                amp: 10u64,
                owner: None,
            })
            .unwrap(),
        ),
        PairType::Custom(t) => match t.as_str() {
            "concentrated" => Some(to_json_binary(&common_pcl_params()).unwrap()),
            _ => None,
        },
        _ => None,
    };
    let (pair_addr, lp_token_addr) = create_astroport_pair(
        runner,
        &astroport_contracts.factory.address,
        pool_type,
        [astro_asset_infos[0].clone(), astro_asset_infos[1].clone()],
        init_params,
        admin,
        None,
    );

    // Increase allowance of CW20's for Pair contract
    for asset in asset_list.into_iter() {
        if let AssetInfoBase::Cw20(cw20_addr) = &asset.info {
            let increase_allowance_msg = Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_addr.clone(),
                amount: asset.amount,
                expires: None,
            };
            let _res = wasm
                .execute(cw20_addr.as_ref(), &increase_allowance_msg, &[], admin)
                .unwrap();
        }
    }

    // Add initial pool liquidity
    let provide_liq_msg = PairExecuteMsg::ProvideLiquidity {
        assets: astro_assets,
        slippage_tolerance: Some(Decimal::from_str("0.02").unwrap()),
        auto_stake: Some(false),
        receiver: None,
    };
    let (native_coins, _) = separate_natives_and_cw20s(&asset_list);
    let _res = wasm
        .execute(&pair_addr, &provide_liq_msg, &native_coins, admin)
        .unwrap();

    // Upload test contract wasm file
    let contract = match &runner {
        TestRunner::MultiTest(_) => ContractType::MultiTestContract(Box::new(
            ContractWrapper::new_with_empty(
                astroport_test_contract::contract::execute,
                astroport_test_contract::contract::instantiate,
                astroport_test_contract::contract::query,
            )
            .with_reply(astroport_test_contract::contract::reply),
        )),
        #[cfg(feature = "osmosis-test-tube")]
        TestRunner::OsmosisTestApp(_) => {
            ContractType::Artifact(Artifact::Local(wasm_file_path.to_string()))
        }
        _ => panic!("Unsupported test runner"),
    };
    let code_id = upload_wasm_file(runner, &accs[0], contract).unwrap();

    // Instantiate the test contract
    let contract_addr = instantiate_test_astroport_contract(
        runner,
        code_id,
        pair_addr.clone(),
        astroport_contracts.incentives.address,
        AssetInfo::cw20(Addr::unchecked(astroport_contracts.astro_token.address)),
        lp_token_addr.clone(),
        astroport_contracts.liquidity_manager.address,
        &accs[0],
    )?;

    Ok((accs, lp_token_addr, pair_addr, contract_addr, asset_list))
}

#[allow(clippy::too_many_arguments)]
pub fn instantiate_test_astroport_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pair_addr: String,
    incentives_addr: String,
    astro_token: AssetInfo,
    lp_token_addr: String,
    liquidity_manager_addr: String,
    signer: &SigningAccount,
) -> RunnerResult<String> {
    let init_msg = AstroportContractInstantiateMsg {
        pair_addr,
        lp_token_addr,
        incentives_addr,
        astro_token,
        liquidity_manager_addr,
    };

    let wasm = Wasm::new(runner);
    Ok(wasm
        .instantiate(code_id, &init_msg, None, None, &[], signer)?
        .data
        .address)
}
