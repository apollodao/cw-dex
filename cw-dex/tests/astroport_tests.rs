use apollo_utils::coins::coin_from_str;
use apollo_utils::submessages::{find_event, parse_attribute_value};
use astroport::asset::AssetInfo as AstroAssetInfo;
use astroport::pair::SimulationResponse;
use cosmwasm_std::{coins, Addr, SubMsgResponse, Uint128};
use cw20::{Cw20Coin, MinterResponse};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_asset::astroport::AstroAsset;
use cw_asset::{AssetBase, AssetInfo, AssetList};
use cw_dex::implementations::astroport::msg::PairQueryMsg;
use cw_dex_test_contract::msg::{AstroportExecuteMsg, ExecuteMsg, QueryMsg};
use cw_dex_test_helpers::astroport::{setup_pool_and_test_contract, AstroportPoolType};
use cw_dex_test_helpers::{
    cw20_balance_query, cw20_mint, cw20_transfer, instantiate_cw20, provide_liquidity,
};
use cw_it::astroport::AstroportContracts;
use cw_it::config::TestConfig;
use cw_it::helpers::bank_send;
use osmosis_testing::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
use osmosis_testing::{
    Account, ExecuteResponse, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount, Wasm,
};
use test_case::test_case;

const HUNDRED_TRILLION: Uint128 = Uint128::new(100_000_000_000_000);
const INITIAL_TWO_POOL_LIQUIDITY: &[u64] = &[1_000_000_000, 1_000_000_000];

const TEST_CONTRACT_WASM_FILE_PATH: &str =
    "../target/wasm32-unknown-unknown/release/astroport_test_contract.wasm";

fn setup_pool_and_contract(
    added_liquidity: Vec<(&str, u64)>,
    initial_liquidity: Vec<u64>,
) -> RunnerResult<(
    OsmosisTestApp,
    Vec<SigningAccount>,
    AstroportContracts,
    String,
    String,
    String,
    AssetList,
)> {
    setup_pool_and_test_contract(
        added_liquidity,
        initial_liquidity,
        TEST_CONTRACT_WASM_FILE_PATH,
    )
}

#[test_case(AstroportPoolType::Basic { }, vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(999000) ; "provide_liquidity: native-cw20")]
#[test_case(AstroportPoolType::Basic { }, vec![("apollo",1_000_000), ("astro", 1_000_000)], Uint128::new(999000); "provide_liquidity: cw20-cw20")]
//#[test_case(AstroportPoolType::Basic { }, vec![("uluna",1_000_000),
//#[test_case(AstroportPoolType::Basic ("uusd",1_000_000)], false,
//#[test_case(AstroportPoolType::Basic Uint128::new(999000)  ; "basic pool:
//#[test_case(AstroportPoolType::Basic native-native")]
// "basic pool adding small liquidity")] #[test_case(AstroportPoolType::Basic,
// vec![1_000_000, 1_000_000], true, INITIAL_LIQUIDITY * HUNDRED_TRILLION ;
// "basic pool simulate min_out")] #[test_case(AstroportPoolType::Basic,
// vec![1_000_000, 500_000], true, Uint128::new(500_000) * HUNDRED_TRILLION ;
// "basic pool uneven assets simulate min_out")]
pub fn test_provide_liquidity(
    pool_type: AstroportPoolType,
    added_liquidity: Vec<(&str, u64)>,
    expected_lps: Uint128,
) {
    let initial_liquidity = added_liquidity.iter().map(|_| 1_000_000).collect();
    let (runner, accs, astroport_contracts, lp_token_addr, pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(added_liquidity, initial_liquidity).unwrap();
    let admin = &accs[0];

    let lp_token_before =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();

    assert_eq!(lp_token_before, Uint128::zero());

    // Provide liquidity
    provide_liquidity(
        &runner,
        contract_addr.clone(),
        asset_list,
        Uint128::one(),
        admin,
    );

    // Query LP token balance after
    let lp_token_after = cw20_balance_query(&runner, lp_token_addr, contract_addr).unwrap();

    assert_eq!(lp_token_after, expected_lps);
}

#[test_case(AstroportPoolType::Basic { }, vec![("uluna",1_000_000), ("astro", 1_000_000)] ; "withdraw_liquidity: native-cw20")]
#[test_case(AstroportPoolType::Basic { }, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: cw20-cw20")]
fn test_withdraw_liquidity(pool_type: AstroportPoolType, added_liquidity: Vec<(&str, u64)>) {
    let initial_liquidity = added_liquidity.iter().map(|_| 1_000_000).collect();

    let (runner, accs, astroport_contracts, lp_token_addr, pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(added_liquidity.clone(), initial_liquidity).unwrap();
    let admin = &accs[0];

    // Send LP tokens to the test contract
    provide_liquidity(
        &runner,
        contract_addr.clone(),
        asset_list,
        Uint128::one(),
        admin,
    );

    //Query admin LP token balance
    let admin_lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), admin.address().clone()).unwrap();

    println!("admin_lp_token_balance: {}", admin_lp_token_balance);

    let contract_lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();

    println!("contract_lp_token_balance: {}", contract_lp_token_balance);

    // cw20_transfer(
    //     &runner,
    //     lp_token_addr.clone(),
    //     contract_addr.clone(),
    //     admin_lp_token_balance,
    //     admin,
    // )
    // .unwrap();

    // Withdraw liquidity. We are not allowed to withdraw all liquidity on osmosis.
    let withdraw_msg = ExecuteMsg::WithdrawLiquidity {
        amount: contract_lp_token_balance,
    };

    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[withdraw_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
            admin,
        )
        .unwrap();

    // Query LP token balance after
    let lp_token_balance_after = cw20_balance_query(&runner, lp_token_addr, contract_addr).unwrap();

    // Assert that LP token balance is 1
    assert_eq!(lp_token_balance_after, Uint128::zero());
}

fn stake_all_lp_tokens<'a, R: Runner<'a>>(
    runner: &'a R,
    contract_addr: String,
    lp_token_addr: String,
    signer: &SigningAccount,
) -> ExecuteResponse<MsgExecuteContractResponse> {
    // Query LP token balance
    let lp_token_balance =
        cw20_balance_query(runner, lp_token_addr, contract_addr.clone()).unwrap();

    // Stake LP tokens
    let stake_msg = ExecuteMsg::Stake {
        amount: lp_token_balance,
    };

    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[stake_msg.into_cosmos_msg(contract_addr, vec![])],
            signer,
        )
        .unwrap()
}

#[test_case(vec![("uluna",1_000_000), ("astro", 1_000_000)] ; "stake_and_unlock: native-cw20")]
#[test_case(vec![("apollo",1_000_000), ("astro", 1_000_000)]; "stake_and_unlock: cw20-cw20")]
fn test_stake_and_unlock(added_liquidity: Vec<(&str, u64)>) -> RunnerResult<()> {
    let initial_liquidity = added_liquidity.iter().map(|_| 1_000_000).collect();
    let (runner, accs, astroport_contracts, lp_token_addr, pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(added_liquidity.clone(), initial_liquidity).unwrap();

    let admin = &accs[0];

    provide_liquidity(
        &runner,
        contract_addr.clone(),
        asset_list,
        Uint128::one(),
        admin,
    );
    // Query LP token balance
    let lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();

    // Send LP tokens to the test contract

    // Stake LP tokens
    let events =
        stake_all_lp_tokens(&runner, contract_addr.clone(), lp_token_addr.clone(), admin).events;

    // Parse the event data
    let response = SubMsgResponse { events, data: None };

    let event = find_event(&response, "wasm").unwrap();
    let amount = coin_from_str(&parse_attribute_value::<String, _>(event, "amount").unwrap());

    // Assert the lock has correct amount
    assert_eq!(amount.amount, lp_token_balance);

    // Query LP token balance after
    let lp_token_balance_after =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.to_string()).unwrap();

    // Assert that LP token balance is 0
    assert_eq!(lp_token_balance_after, Uint128::zero());

    // Unlock LP tokens
    let unlock_msg = AstroportExecuteMsg::Unstake {
        amount: lp_token_balance,
    };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[unlock_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
            admin,
        )
        .unwrap();

    // Query LP token balance
    let lp_token_balance_after_unlock =
        cw20_balance_query(&runner, lp_token_addr, contract_addr.to_string()).unwrap();

    // Assert that LP tokens have been unlocked
    assert_eq!(lp_token_balance_after_unlock, lp_token_balance);

    Ok(())
}

#[test_case(AstroportPoolType::Basic{},vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(1_000_000) ; "swap_and_simulate_swap: basic pool")]
#[test_case(AstroportPoolType::Basic{},vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(2) ; "swap_and_simulate_swap: basic pool small amount")]
#[test_case(AstroportPoolType::Basic{},vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(1000000) ; "swap_and_simulate_swap: basic pool with min out")]
fn test_swap_and_simulate_swap(
    pool_type: AstroportPoolType,
    added_liquidity: Vec<(&str, u64)>,
    amount: Uint128,
) {
    let initial_liquidity = added_liquidity.iter().map(|_| 1_000_000).collect();

    let (runner, accs, astroport_contracts, lp_token_addr, pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(added_liquidity, initial_liquidity).unwrap();

    let admin = &accs[0];

    println!("lp_token_addr {}", lp_token_addr);
    // Simulate swap
    let offer = &asset_list.clone().to_vec()[0];
    let ask = &asset_list.clone().to_vec()[1];
    let wasm = Wasm::new(&runner);
    let simulate_query = QueryMsg::SimulateSwap {
        offer: offer.clone(),
        ask: ask.info.clone(),
        sender: None,
    };

    let expected_out = wasm.query(&contract_addr, &simulate_query).unwrap();

    // Swap
    let swap_msg = ExecuteMsg::Swap {
        offer: offer.clone(),
        ask: ask.info.clone(),
        min_out: expected_out,
    };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[swap_msg.into_cosmos_msg(
                contract_addr.clone(),
                vec![offer.clone().try_into().unwrap()],
            )],
            admin,
        )
        .unwrap();

    // Query OSMO and ATOM balances
    let offer_balance =
        cw20_balance_query(&runner, contract_addr.to_string(), offer.info.to_string()).unwrap();
    let ask_balance = cw20_balance_query(&runner, contract_addr, ask.info.to_string()).unwrap();

    // Assert that OSMO and ATOM balances are correct
    assert_eq!(ask_balance, expected_out);
    assert_eq!(offer_balance, Uint128::zero());
}
