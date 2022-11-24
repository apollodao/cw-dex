use cosmwasm_std::Coin;
use integration_tests::app::App;
use integration_tests::application::Application;
use integration_tests::chain::Chain;
use integration_tests::module::osmosis::{Gamm, TokenFactory};
use integration_tests::module::Module;
use integration_tests::result::ExecuteResponse;
use integration_tests::runner::Runner;
use integration_tests_config::account::Account;
use integration_tests_config::config::TestConfig;
use test_case::test_case;

use cosmrs::proto::cosmos::auth::v1beta1::BaseAccount;
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgCreateDenom, MsgCreateDenomResponse, MsgMint, QueryDenomsFromCreatorRequest,
    QueryParamsRequest,
};
use testcontainers::clients::Cli;
use testcontainers::images::generic::GenericImage;
use testcontainers::{Container, RunnableImage};

#[test_case("osmosis.yaml" ; "osmosis pool creation")]
fn create_osmosis_pool(cfg_path: &str) {
    // Init
    let mut config: TestConfig = TestConfig::from_yaml(cfg_path);
    let osmosis = App::default();
    let docker: Cli = Cli::default();

    // Given a running local Chain
    let container: Container<GenericImage> = docker.run(RunnableImage::from(
        osmosis.get_image(&config.container.name, &config.container.tag),
    ));
    config.bind_chain_to_container(&container);
    let chain = Chain::new(&config.chain_cfg);

    let gamm = Gamm::new(&osmosis);
    let token_factory = TokenFactory::new(&osmosis);

    // and an admin account
    let admin = config.import_account("validator").unwrap();

    // and created Token "apollo"
    let create_denom_msg = MsgCreateDenom {
        sender: admin.address(),
        subdenom: "uapollo".to_string(),
    };
    let resp = token_factory
        .create_denom(&chain, create_denom_msg, &admin)
        .unwrap();

    let new_token_denom = resp.data.new_token_denom;
    // TODO: check events

    let mint_msg = MsgMint {
        sender: admin.address(),
        amount: Some(osmosis_std::types::cosmos::base::v1beta1::Coin {
            amount: "1000".to_string(),
            denom: new_token_denom.clone(),
        }),
    };
    let _mint_response = token_factory.mint(&chain, mint_msg, &admin).unwrap().data;

    let query_denom_msg = QueryDenomsFromCreatorRequest {
        creator: admin.address(),
    };
    let query_denoms_response = token_factory
        .query_denoms_from_creator(&chain, &query_denom_msg)
        .unwrap()
        .denoms;
    println!("Query Denoms from creator [{:#?}]", query_denoms_response);

    // When we create the pool uapollo-uosmo
    let pool_liquidity = vec![Coin::new(1_000, new_token_denom), Coin::new(1_000, "uosmo")];
    let pool_id = gamm
        .create_basic_pool(&chain, &pool_liquidity, &admin)
        .unwrap()
        .data
        .pool_id;

    println!("pool_id [{:?}]", pool_id);
    assert_eq!(1u64, pool_id);
    let pool = gamm.query_pool(&chain, pool_id).unwrap();
    for asset in pool.pool_assets.clone() {
        println!("PoolAsset [{:?}]", asset);
    }

    let pool_assets: Vec<osmosis_std::types::cosmos::base::v1beta1::Coin> = pool
        .pool_assets
        .into_iter()
        .map(|pool_asset| pool_asset.token.unwrap())
        .collect();

    assert_eq!(pool_liquidity[0].denom, pool_assets[0].denom);
    assert_eq!(pool_liquidity[1].denom, pool_assets[1].denom);
    // TODO: make a deposit to check if balance change
}

#[test_case("osmosis.yaml" ; "osmosis chain execute")]
fn test_execute(cfg_path: &str) {
    // Init
    let mut config: TestConfig = TestConfig::from_yaml(cfg_path);
    let osmosis = App::default();
    let docker: Cli = Cli::default();

    // Given a running local Chain
    let container: Container<GenericImage> = docker.run(RunnableImage::from(
        osmosis.get_image(&config.container.name, &config.container.tag),
    ));
    config.bind_chain_to_container(&container);
    let chain = Chain::new(&config.chain_cfg);

    // init modules
    let token_factory = TokenFactory::new(&osmosis);

    // init accounts
    let acc = config.import_account("test1").unwrap();
    let addr = acc.address();

    let msg: MsgCreateDenom = MsgCreateDenom {
        sender: addr.clone(),
        subdenom: "Apollo".to_string(),
    };
    let res = token_factory.create_denom(&chain, msg, &acc).unwrap();
    assert_eq!(
        res.data.new_token_denom,
        format!("factory/{}/{}", &addr, "Apollo")
    );

    let query_params_msg: QueryParamsRequest = QueryParamsRequest {};

    let resp = token_factory
        .query_params(&chain, &query_params_msg)
        .unwrap();

    let denom_creation_fee = resp.params.unwrap().denom_creation_fee;

    assert_eq!(
        denom_creation_fee,
        [Coin::new(10_000_000, chain.chain_cfg().denom()).into()]
    );

    // execute on more time to excercise account sequence
    let msg = MsgCreateDenom {
        sender: acc.address(),
        subdenom: "newerdenom".to_string(),
    };

    let res: ExecuteResponse<MsgCreateDenomResponse> = osmosis
        .execute(&chain, msg, MsgCreateDenom::TYPE_URL, &acc)
        .unwrap();

    assert_eq!(
        res.data.new_token_denom,
        format!("factory/{}/{}", &addr, "newerdenom")
    );
    let account: BaseAccount = osmosis.account(&chain, acc.account_id()).unwrap();
    assert_eq!(account.sequence, 2);
}

#[test_case("osmosis.yaml" ; "osmosis module")]
fn test_multiple_as_module(cfg_path: &str) {
    // Init
    let mut config: TestConfig = TestConfig::from_yaml(cfg_path);
    let osmosis = App::default();
    let docker: Cli = Cli::default();

    // Given a running local Chain
    let container: Container<GenericImage> = docker.run(RunnableImage::from(
        osmosis.get_image(&config.container.name, &config.container.tag),
    ));
    config.bind_chain_to_container(&container);
    let chain = Chain::new(&config.chain_cfg);

    // and Osmosis modules
    let gamm = Gamm::new(&osmosis);
    let token_factory = TokenFactory::new(&osmosis);

    // and an admin account
    let admin = config.import_account("test1").unwrap();

    // and created Token "apollo"
    let create_denom_msg = MsgCreateDenom {
        sender: admin.address(),
        subdenom: "uapollo".to_string(),
    };
    let new_token_denom = token_factory
        .create_denom(&chain, create_denom_msg, &admin)
        .unwrap()
        .data
        .new_token_denom;
    // TODO: check events

    let query_params_msg: QueryParamsRequest = QueryParamsRequest {};

    let resp = token_factory
        .query_params(&chain, &query_params_msg)
        .unwrap();

    // println!("Resp[{:?}]", resp);
    let denom_creation_fee = resp.params.unwrap().denom_creation_fee;

    assert_eq!(
        denom_creation_fee,
        [Coin::new(10_000_000, chain.chain_cfg().denom()).into()]
    );

    let mint_msg = MsgMint {
        sender: admin.address(),
        amount: Some(osmosis_std::types::cosmos::base::v1beta1::Coin {
            amount: "1000".to_string(),
            denom: new_token_denom.clone(),
        }),
    };
    let _mint_response = token_factory.mint(&chain, mint_msg, &admin).unwrap().data;

    let query_denom_msg = QueryDenomsFromCreatorRequest {
        creator: admin.address(),
    };
    let query_denoms_response = token_factory
        .query_denoms_from_creator(&chain, &query_denom_msg)
        .unwrap()
        .denoms;
    println!("Query Denoms from creator [{:#?}]", query_denoms_response);

    // When we create the pool uapollo-uosmo
    let pool_liquidity = vec![Coin::new(1_000, new_token_denom), Coin::new(1_000, "uosmo")];
    let pool_id = gamm
        .create_basic_pool(&chain, &pool_liquidity, &admin)
        .unwrap()
        .data
        .pool_id;

    println!("pool_id [{:?}]", pool_id);
    assert_eq!(1u64, pool_id);
    let pool = gamm.query_pool(&chain, pool_id).unwrap();
    for asset in pool.pool_assets.clone() {
        println!("PoolAsset [{:?}]", asset);
    }

    let pool_assets: Vec<osmosis_std::types::cosmos::base::v1beta1::Coin> = pool
        .pool_assets
        .into_iter()
        .map(|pool_asset| pool_asset.token.unwrap())
        .collect();

    assert_eq!(pool_liquidity[0].denom, pool_assets[0].denom);
    assert_eq!(pool_liquidity[1].denom, pool_assets[1].denom);
    // TODO: make a deposit to check if pool assets relationship change
}