use astroport::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport::factory::{ExecuteMsg as FactoryExecuteMsg, PairType, PairsResponse};
use cosmwasm_std::{coins, Addr, Coin, Uint128};
use cw20::{Cw20Coin, MinterResponse};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_asset::{AssetBase, AssetInfo, AssetInfoBase, AssetList};
use cw_it::astroport::{
    create_astroport_pair, instantiate_astroport, upload_astroport_contracts, AstroportContracts,
};
use cw_it::config::TestConfig;
use cw_it::helpers::{bank_send, upload_wasm_file};
use osmosis_testing::{Account, OsmosisTestApp, RunnerResult, SigningAccount};

use crate::{
    cw20_mint, instantiate_cw20, instantiate_test_astroport_contract, instantiate_test_contract,
};

pub enum AstroportPoolType {
    Basic {},
    StableSwap { scaling_factors: Vec<u64> },
}

const TEST_CONFIG_PATH: &str = "tests/configs/terra.yaml";

/// Setup a pool and test contract for testing.
pub fn setup_pool_and_test_contract(
    added_liquidity: Vec<(&str, u64)>,
    initial_liquidity: Vec<u64>,
    wasm_file_path: &str,
) -> RunnerResult<(
    OsmosisTestApp,
    Vec<SigningAccount>,
    AstroportContracts,
    String,
    String,
    String,
    AssetList,
)> {
    let runner = OsmosisTestApp::new();
    let test_config = TestConfig::from_yaml(TEST_CONFIG_PATH);

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
        denom: "uluna".to_string(),
        amount: Uint128::MAX,
    });

    // Initialize 10 accounts with max balance of each token in the pool
    let accs = runner.init_accounts(&initial_balances, 10).unwrap();

    let admin = &accs[0];

    let astroport_code_ids = upload_astroport_contracts(&runner, &test_config, admin);

    // Instantiate Astroport contracts
    let astroport_contracts = instantiate_astroport(&runner, admin, &astroport_code_ids);

    let mut asset_list = AssetList::new();
    let mut apollo_token: String = "".to_string();
    for asset in added_liquidity.iter() {
        if asset.0 == "astro" {
            asset_list
                .add(&AssetBase::new(
                    AssetInfo::Cw20(Addr::unchecked(
                        astroport_contracts.clone().astro_token.address,
                    )),
                    Uint128::new(asset.1.into()),
                ))
                .unwrap();
        } else if asset.0 == "uluna" {
            asset_list
                .add(&AssetBase::new(
                    AssetInfo::Native("uluna".to_string()),
                    Uint128::new(asset.1.into()),
                ))
                .unwrap();
        } else if asset.0 == "apollo" {
            apollo_token = instantiate_cw20(
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
                &admin,
            )
            .unwrap();
            asset_list
                .add(&AssetBase::new(
                    AssetInfo::Cw20(Addr::unchecked(apollo_token.clone())),
                    Uint128::new(asset.1.into()),
                ))
                .unwrap();
        }
    }

    let mut astro_asset_infos = vec![];
    for asset in asset_list.into_iter() {
        match &asset.info {
            AssetInfoBase::Cw20(addr) => astro_asset_infos.push(AstroAssetInfo::Token {
                contract_addr: addr.to_owned(),
            }),
            AssetInfoBase::Native(denom) => astro_asset_infos.push(AstroAssetInfo::NativeToken {
                denom: denom.to_string(),
            }),
        }
    }

    let astro_asset_conv: [AstroAssetInfo; 2] = astro_asset_infos.try_into().unwrap();

    println!("astro_asset_conv: {:?}", astro_asset_conv);

    // Create pool
    let (pair_addr, lp_token_addr) = create_astroport_pair(
        &runner,
        &astroport_contracts.factory.address,
        PairType::Xyk {},
        astro_asset_conv,
        None,
        admin,
    );

    // Upload test contract wasm file
    let code_id = upload_wasm_file(&runner, &accs[0], wasm_file_path).unwrap();

    // Instantiate the test contract
    let contract_addr = instantiate_test_astroport_contract(
        &runner,
        code_id,
        pair_addr.clone(),
        astroport_contracts.clone().generator.address,
        astroport_contracts.clone().astro_token.address,
        lp_token_addr.clone(),
        &accs[0],
    )?;

    for asset in added_liquidity.iter() {
        if asset.0 == "astro" {
            cw20_mint(
                &runner,
                astroport_contracts.clone().astro_token.address,
                contract_addr.clone(),
                asset.1.into(),
                admin,
            )
            .unwrap();
        } else if asset.0 == "uluna" {
            bank_send(
                &runner,
                admin,
                &contract_addr.clone(),
                coins(asset.1.into(), asset.0),
            )
            .unwrap();
        } else if asset.0 == "apollo" {
            cw20_mint(
                &runner,
                apollo_token.clone(),
                contract_addr.clone(),
                asset.1.into(),
                admin,
            )
            .unwrap();
        }
    }

    Ok((
        runner,
        accs,
        astroport_contracts,
        lp_token_addr,
        pair_addr,
        contract_addr,
        asset_list,
    ))
}
