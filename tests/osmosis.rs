use cw_asset::Asset;
use integration_tests_config::account::Account;
use cw_asset::AssetInfo;
use cosmwasm_std::Uint128;
use cw_dex_test::msg::ExecuteMsg;
use cw_dex_test::msg::InstantiateMsg;
use integration_tests_core::module::Wasm;
use integration_tests_core::contract::Contract;
use integration_tests_core::module::Module;
use cw_asset::AssetList;
use integration_tests_core::module::osmosis::TokenFactory;
use integration_tests_core::module::osmosis::Gamm;
use integration_tests_core::test_setup;
use cosmwasm_std::Coin;

use test_case::test_case;

use osmosis_std::types::osmosis::tokenfactory::v1beta1::{ MsgCreateDenom, MsgMint };
use testcontainers::clients::Cli;

#[test_case("./tests/osmosis.yaml" ; "osmosis pool creation")]
fn create_osmosis_pool(cfg_path: &str) {
    // Init
    let docker: Cli = Cli::default();
    let (_config, app, _container, chain, accs) = test_setup!(docker, cfg_path);

    let gamm = Gamm::new(&app);
    let token_factory = TokenFactory::new(&app);
    let wasm = Wasm::new(&app);

    // and an admin account
    let admin = &accs["validator"];

    // and created Token
    let create_denom_msg = MsgCreateDenom {
        sender: admin.address(),
        subdenom: "udummy".to_string(),
    };
    let dummy_token = token_factory
        .create_denom(&chain, create_denom_msg, &admin)
        .unwrap().data.new_token_denom;

    let mint_msg = MsgMint {
        sender: admin.address(),
        amount: Some(osmosis_std::types::cosmos::base::v1beta1::Coin {
            amount: "1000".to_string(),
            denom: dummy_token.clone(),
        }),
    };
    let _mint_response = token_factory.mint(&chain, mint_msg, &admin).unwrap().data;

    // When we create the pool udummy-uosmo
    let pool_liquidity = vec![Coin::new(1_000, dummy_token), Coin::new(1_000, "uosmo")];
    let pool_id = gamm.create_basic_pool(&chain, &pool_liquidity, &admin).unwrap().data.pool_id;

    // TODO: Use here the smart contract that will use the Pool
    let contract: Contract = Contract::store(&app, &chain, &admin, "cw_dex_test");
    let contract_addr = wasm
        .instantiate(&chain, contract.code_id, &(InstantiateMsg {}), None, None, &[], &admin)
        .unwrap().data.address;
    println!("Contract Initialized [{}]", contract_addr);

    let mut assets = AssetList::new();
    assets.add(&Asset::native("uosmo", 1u128)).unwrap();
    assets.add(&Asset::native("udummy", 1u128)).unwrap();
    let response = wasm
        .execute::<ExecuteMsg>(
            &chain,
            &contract_addr,
            &(ExecuteMsg::ProvideLiquidity { pool_id, assets, min_out: Uint128::zero() }),
            &[],
            &admin
        )
        .unwrap();
    println!("Resp [{:#?}]", response);
    assert!(false);
}