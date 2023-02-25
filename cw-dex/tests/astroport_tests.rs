use apollo_cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use apollo_utils::coins::coin_from_str;
use apollo_utils::submessages::{find_event, parse_attribute_value};
use astroport_types::factory::PairType;
use astroport_types::pair::{PoolResponse, QueryMsg as PairQueryMsg};
use cosmwasm_std::{Coin, Decimal, SubMsgResponse, Uint128};
use cw_dex_test_contract::msg::{AstroportExecuteMsg, ExecuteMsg, QueryMsg};
use cw_dex_test_helpers::astroport::setup_pool_and_test_contract;
use cw_dex_test_helpers::{
    cw20_balance_query, cw20_transfer, provide_liquidity, query_asset_balance,
};
use cw_it::osmosis_test_tube::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
use cw_it::osmosis_test_tube::{
    Account, ExecuteResponse, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount, Wasm,
};
use test_case::test_case;

const TEST_CONTRACT_WASM_FILE_PATH: &str =
    "../target/wasm32-unknown-unknown/release/astroport_test_contract.wasm";

fn setup_pool_and_contract(
    pool_type: PairType,
    initial_liquidity: Vec<(&str, u64)>,
) -> RunnerResult<(
    OsmosisTestApp,
    Vec<SigningAccount>,
    String,
    String,
    String,
    AssetList,
)> {
    setup_pool_and_test_contract(
        pool_type,
        initial_liquidity,
        2,
        TEST_CONTRACT_WASM_FILE_PATH,
    )
}

#[test_case(PairType::Xyk { }, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: native-cw20")]
#[test_case(PairType::Xyk { }, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: cw20-cw20")]
#[test_case(PairType::Stable { }, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: stableswap native-cw20")]
#[test_case(PairType::Stable { }, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: stableswap cw20-cw20")]
pub fn test_provide_liquidity(pool_type: PairType, initial_liquidity: Vec<(&str, u64)>) {
    let (runner, accs, lp_token_addr, _pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(pool_type, initial_liquidity).unwrap();
    let admin = &accs[0];

    // Check contract's LP token balance before providing liquidity
    let lp_token_before =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();
    assert_eq!(lp_token_before, Uint128::zero());

    // Provide liquidity
    provide_liquidity(
        &runner,
        contract_addr.clone(),
        asset_list.clone(),
        Uint128::one(),
        admin,
    );

    // Query LP token balance after
    let lp_token_after = cw20_balance_query(&runner, lp_token_addr, contract_addr.clone()).unwrap();
    assert_ne!(lp_token_after, Uint128::zero());

    // Query asset balances in contract, assert that all were used
    for asset in asset_list.into_iter() {
        let asset_balance = query_asset_balance(&runner, &asset.info, &contract_addr);
        assert_eq!(asset_balance, Uint128::zero());
    }
}

#[test_case(PairType::Xyk { }, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: xyk native-cw20")]
#[test_case(PairType::Xyk { }, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: xyk cw20-cw20")]
#[test_case(PairType::Stable { }, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: stableswap native-cw20")]
#[test_case(PairType::Stable { }, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: stableswap cw20-cw20")]
fn test_withdraw_liquidity(pool_type: PairType, initial_liquidity: Vec<(&str, u64)>) {
    let (runner, accs, lp_token_addr, pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(pool_type, initial_liquidity).unwrap();
    let admin = &accs[0];
    let wasm = Wasm::new(&runner);

    //Query admin LP token balance
    let admin_lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), admin.address()).unwrap();
    let amount_to_send = admin_lp_token_balance / Uint128::from(2u128);

    // Send LP tokens to contract
    cw20_transfer(
        &runner,
        lp_token_addr.clone(),
        contract_addr.clone(),
        amount_to_send,
        admin,
    )
    .unwrap();
    let contract_lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();
    assert_eq!(contract_lp_token_balance, amount_to_send);

    // Query pool info
    let pool_res = wasm
        .query::<_, PoolResponse>(&pair_addr, &PairQueryMsg::Pool {})
        .unwrap();
    let lp_token_ratio = Decimal::from_ratio(amount_to_send, pool_res.total_share);

    // Withdraw liquidity
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
    let lp_token_balance_after =
        cw20_balance_query(&runner, lp_token_addr, contract_addr.clone()).unwrap();

    // Assert that LP token balance is zero after withdrawing all liquidity
    assert_eq!(lp_token_balance_after, Uint128::zero());

    // Query contract asset balances, assert that all were returned
    for asset in asset_list.into_iter() {
        let asset_balance = query_asset_balance(&runner, &asset.info, &contract_addr);
        let expected_balance = pool_res
            .assets
            .iter()
            .find(|a| AssetInfo::from(a.info.clone()) == asset.info)
            .unwrap()
            .amount
            * lp_token_ratio;
        assert_eq!(asset_balance, expected_balance);
    }
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

#[test_case(PairType::Xyk {}, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: xyk native-cw20")]
#[test_case(PairType::Xyk {}, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: xyk cw20-cw20")]
#[test_case(PairType::Stable {}, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: stableswap native-cw20")]
#[test_case(PairType::Stable {}, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: stableswap cw20-cw20")]
fn test_stake_and_unstake(
    pool_type: PairType,
    initial_liquidity: Vec<(&str, u64)>,
) -> RunnerResult<()> {
    let (runner, accs, lp_token_addr, _pair_addr, contract_addr, _asset_list) =
        setup_pool_and_contract(pool_type, initial_liquidity).unwrap();

    let admin = &accs[0];

    // Query LP token balance
    let lp_token_balance =
        cw20_balance_query(&runner, lp_token_addr.clone(), admin.address()).unwrap();

    // Send LP tokens to the test contract
    cw20_transfer(
        &runner,
        lp_token_addr.clone(),
        contract_addr.clone(),
        lp_token_balance,
        admin,
    )
    .unwrap();

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

    // unstake LP tokens
    let unstake_msg = AstroportExecuteMsg::Unstake {
        amount: lp_token_balance,
    };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[unstake_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
            admin,
        )
        .unwrap();

    // Query LP token balance
    let lp_token_balance_after_unstake =
        cw20_balance_query(&runner, lp_token_addr, contract_addr).unwrap();

    // Assert that LP tokens have been unstakeed
    assert_eq!(lp_token_balance_after_unstake, lp_token_balance);

    Ok(())
}

#[test_case(PairType::Xyk{},vec![("astro",1_000_000), ("uluna", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: basic pool")]
#[test_case(PairType::Xyk{},vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(2); "swap_and_simulate_swap: basic pool small amount")]
#[test_case(PairType::Xyk{},vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: basic pool, high slippage")]
#[test_case(PairType::Xyk{},vec![("uluna",68_582_147), ("astro", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: basic pool, random prices")]
#[test_case(PairType::Stable { },vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: stable swap pool")]
#[test_case(PairType::Stable { },vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: stable swap pool, high slippage")]
#[test_case(PairType::Stable { },vec![("uluna",68_582_147), ("astro", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: stable swap pool, random prices")]
fn test_swap_and_simulate_swap(
    pool_type: PairType,
    initial_liquidity: Vec<(&str, u64)>,
    amount: Uint128,
) {
    let (runner, accs, _lp_token_addr, _pair_addr, contract_addr, asset_list) =
        setup_pool_and_contract(pool_type, initial_liquidity).unwrap();

    let admin = &accs[0];
    let wasm = Wasm::new(&runner);

    let offer_info = &asset_list.to_vec()[0].info;
    let ask_info = &asset_list.to_vec()[1].info;

    // Simulate swap
    let offer = Asset {
        info: offer_info.clone(),
        amount,
    };
    let simulate_query = QueryMsg::SimulateSwap {
        offer: offer.clone(),
        ask: ask_info.clone(),
        sender: None,
    };

    let expected_out = wasm.query(&contract_addr, &simulate_query).unwrap();

    // Swap
    let swap_msg = ExecuteMsg::Swap {
        offer: offer.clone(),
        ask: ask_info.clone(),
        min_out: expected_out,
    };
    let native_coins = match offer.info {
        AssetInfoBase::Native(denom) => {
            vec![Coin {
                denom,
                amount: offer.amount,
            }]
        }
        AssetInfoBase::Cw20(cw20_addr) => {
            // Transfer cw20 tokens to the contract
            cw20_transfer(
                &runner,
                cw20_addr.to_string(),
                contract_addr.clone(),
                offer.amount,
                admin,
            )
            .unwrap();
            vec![]
        }
    };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[swap_msg.into_cosmos_msg(contract_addr.clone(), native_coins)],
            admin,
        )
        .unwrap();

    // Query offer and ask balances
    let offer_balance = query_asset_balance(&runner, offer_info, &contract_addr);
    let ask_balance = query_asset_balance(&runner, ask_info, &contract_addr);

    // Assert that offer and ask balances are correct
    assert_eq!(ask_balance, expected_out);
    assert_eq!(offer_balance, Uint128::zero());
}
