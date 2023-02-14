use apollo_cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use apollo_utils::assets::separate_natives_and_cw20s;
use astroport_types::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport_types::factory::PairType;
use astroport_types::pair::{ExecuteMsg as PairExecuteMsg, StablePoolParams};
use cosmwasm_std::{to_binary, Addr, Coin, Decimal, Uint128};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_dex_test_contract::msg::AstroportContractInstantiateMsg;
use cw_it::astroport::{create_astroport_pair, instantiate_astroport, upload_astroport_contracts};
use cw_it::config::TestConfig;
use cw_it::helpers::upload_wasm_file;
use osmosis_test_tube::{
    Account, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount, Wasm,
};
use std::str::FromStr;

use crate::{cw20_mint, instantiate_cw20};

const TEST_CONFIG_PATH: &str = "tests/configs/terra.yaml";

/// Setup a pool and test contract for testing.
pub fn setup_pool_and_test_contract(
    pool_type: PairType,
    initial_liquidity: Vec<(&str, u64)>,
    native_denom_count: usize,
    wasm_file_path: &str,
) -> RunnerResult<(
    OsmosisTestApp,
    Vec<SigningAccount>,
    String,
    String,
    String,
    AssetList,
)> {
    let runner = OsmosisTestApp::new();
    let wasm = Wasm::new(&runner);
    let test_config = TestConfig::from_yaml(TEST_CONFIG_PATH);

    // Initialize 10 accounts with max balance of each token
    let mut initial_balances = (0..native_denom_count)
        .map(|i| Coin {
            denom: format!("denom{}", i),
            amount: Uint128::MAX,
        })
        .collect::<Vec<_>>();
    initial_balances.push(Coin {
        denom: "uluna".to_string(),
        amount: Uint128::MAX,
    });
    let accs = runner.init_accounts(&initial_balances, 10).unwrap();

    let admin = &accs[0];

    let astroport_code_ids = upload_astroport_contracts(&runner, &test_config, admin);

    // Instantiate Astroport contracts
    let astroport_contracts = instantiate_astroport(&runner, admin, &astroport_code_ids);

    // Instantiate Apollo token (to have second CW20 to test CW20-CW20 pools)
    let apollo_token = instantiate_cw20(
        &runner,
        astroport_code_ids["astro_token"],
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
            &runner,
            astroport_contracts.clone().astro_token.address,
            account.address().clone(),
            Uint128::from(1_000_000_000_000_000_000u128),
            admin,
        )
        .unwrap();
        // Mint Apollo tokens
        cw20_mint(
            &runner,
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
    let init_params = match pool_type {
        PairType::Stable {} => Some(to_binary(&StablePoolParams { amp: 10u64 }).unwrap()),
        _ => None,
    };
    let (pair_addr, lp_token_addr) = create_astroport_pair(
        &runner,
        &astroport_contracts.factory.address,
        pool_type,
        astro_asset_infos.try_into().unwrap(),
        init_params,
        admin,
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
        assets: astro_assets.try_into().unwrap(),
        slippage_tolerance: Some(Decimal::from_str("0.02").unwrap()),
        auto_stake: Some(false),
        receiver: None,
    };
    let (native_coins, _) = separate_natives_and_cw20s(&asset_list);
    let _res = wasm
        .execute(&pair_addr, &provide_liq_msg, &native_coins, admin)
        .unwrap();

    // Upload test contract wasm file
    let code_id = upload_wasm_file(&runner, &accs[0], wasm_file_path).unwrap();

    // Instantiate the test contract
    let contract_addr = instantiate_test_astroport_contract(
        &runner,
        code_id,
        pair_addr.clone(),
        astroport_contracts.clone().generator.address,
        astroport_contracts.astro_token.address,
        lp_token_addr.clone(),
        &accs[0],
    )?;

    Ok((
        runner,
        accs,
        lp_token_addr,
        pair_addr,
        contract_addr,
        asset_list,
    ))
}

pub fn instantiate_test_astroport_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pair_addr: String,
    generator_addr: String,
    astro_addr: String,
    lp_token_addr: String,
    signer: &SigningAccount,
) -> RunnerResult<String> {
    let init_msg = AstroportContractInstantiateMsg {
        pair_addr,
        lp_token_addr,
        generator_addr,
        astro_addr,
    };

    let wasm = Wasm::new(runner);
    Ok(wasm
        .instantiate(code_id, &init_msg, None, None, &[], signer)?
        .data
        .address)
}
