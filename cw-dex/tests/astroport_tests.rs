#![cfg(feature = "astroport")]
mod tests {
    use apollo_cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
    use apollo_utils::assets::separate_natives_and_cw20s;
    use apollo_utils::coins::coin_from_str;
    use apollo_utils::submessages::{find_event, parse_attribute_value};
    use astroport::factory::PairType;
    use cosmwasm_std::{Addr, Coin, SubMsgResponse, Uint128};
    use cw_dex::Pool;
    use cw_dex_test_contract::msg::{AstroportExecuteMsg, ExecuteMsg, QueryMsg};
    use cw_dex_test_helpers::astroport::setup_pool_and_test_contract;
    use cw_dex_test_helpers::{cw20_balance_query, cw20_transfer, query_asset_balance};
    use cw_it::helpers::Unwrap;
    use cw_it::multi_test::MultiTestRunner;
    use cw_it::test_tube::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
    use cw_it::test_tube::{
        Account, ExecuteResponse, Module, Runner, RunnerResult, SigningAccount, Wasm,
    };
    use cw_it::{OwnedTestRunner, TestRunner};
    use test_case::test_case;

    #[cfg(feature = "osmosis-test-tube")]
    use cw_it::osmosis_test_tube::OsmosisTestApp;

    pub fn get_test_runner<'a>() -> OwnedTestRunner<'a> {
        match option_env!("TEST_RUNNER").unwrap_or("multi-test") {
            "multi-test" => OwnedTestRunner::MultiTest(MultiTestRunner::new("osmo")),
            #[cfg(feature = "osmosis-test-tube")]
            "osmosis-test-tube" => OwnedTestRunner::OsmosisTestApp(OsmosisTestApp::new()),
            _ => panic!("Unsupported test runner type"),
        }
    }
    const TEST_CONTRACT_WASM_FILE_PATH: &str =
        "../target/wasm32-unknown-unknown/release/astroport_test_contract.wasm";

    fn setup_pool_and_contract<'a>(
        runner: &'a TestRunner<'a>,
        pool_type: PairType,
        initial_liquidity: Vec<(&str, u64)>,
    ) -> RunnerResult<(Vec<SigningAccount>, String, String, String, AssetList)> {
        setup_pool_and_test_contract(
            runner,
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
    #[test_case(PairType::Stable { }, vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "provide_liquidity: stableswap native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: concentrated native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("apollo",1_000_000), ("astro", 1_000_000)]; "provide_liquidity: concentrated cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "provide_liquidity: concentrated native-native")]
    pub fn test_provide_liquidity(pool_type: PairType, initial_liquidity: Vec<(&str, u64)>) {
        let owned_runner = get_test_runner();
        let runner = owned_runner.as_ref();
        let (accs, lp_token_addr, _pair_addr, contract_addr, asset_list) =
            setup_pool_and_contract(&runner, pool_type.clone(), initial_liquidity).unwrap();
        let admin = &accs[0];
        let wasm = Wasm::new(&runner);

        // Check contract's LP token balance before providing liquidity
        let lp_token_before =
            cw20_balance_query(&runner, lp_token_addr.clone(), contract_addr.clone()).unwrap();
        assert_eq!(lp_token_before, Uint128::zero());

        // Simulate Provide Liquidity. Not supported for concentrated liquidity, so we
        // just make sure to use the right amounts of input assets
        let expected_out = match &pool_type {
            PairType::Custom(_) => Uint128::new(1000000),
            _ => {
                let simulate_query = QueryMsg::SimulateProvideLiquidity {
                    assets: asset_list.clone(),
                };
                wasm.query(&contract_addr, &simulate_query).unwrap()
            }
        };

        let (funds, cw20s) = separate_natives_and_cw20s(&asset_list);

        // Send cw20 tokens to the contract
        for cw20 in cw20s {
            cw20_transfer(
                &runner,
                cw20.address,
                contract_addr.clone(),
                cw20.amount,
                admin,
            )
            .unwrap();
        }

        // Provide liquidity with min_out one more than expected_out. Should fail.
        let unwrap = Unwrap::Err("Slippage is more than expected");
        let min_out = expected_out + Uint128::one();
        let provide_msg = ExecuteMsg::ProvideLiquidity {
            assets: asset_list.clone(),
            min_out,
        };
        unwrap.unwrap(runner.execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[provide_msg.into_cosmos_msg(contract_addr.clone(), funds.clone())],
            admin,
        ));

        // Provide liquidity with expected_out as min_out. Should succeed.
        let provide_msg = ExecuteMsg::ProvideLiquidity {
            assets: asset_list.clone(),
            min_out: expected_out,
        };
        let _res = runner
            .execute_cosmos_msgs::<MsgExecuteContractResponse>(
                &[provide_msg.into_cosmos_msg(contract_addr.clone(), funds)],
                admin,
            )
            .unwrap();

        // Query LP token balance after
        let lp_token_after =
            cw20_balance_query(&runner, lp_token_addr, contract_addr.clone()).unwrap();
        assert_eq!(lp_token_after, expected_out);

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
    #[test_case(PairType::Stable { }, vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "withdraw_liquidity: stableswap native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: concentrated native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("apollo",1_000_000), ("astro", 1_000_000)]; "withdraw_liquidity: concentrated cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "withdraw_liquidity: concentrated native-native")]
    fn test_withdraw_liquidity(pool_type: PairType, initial_liquidity: Vec<(&str, u64)>) {
        let owned_runner = get_test_runner();
        let runner = owned_runner.as_ref();
        let (accs, lp_token_addr, _pair_addr, contract_addr, asset_list) =
            setup_pool_and_contract(&runner, pool_type, initial_liquidity).unwrap();
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

        // Simulate withdraw liquidity to get expected out assets
        let simulate_query = QueryMsg::SimulateWithdrawLiquidty {
            amount: contract_lp_token_balance,
        };
        let expected_out: AssetList = wasm.query(&contract_addr, &simulate_query).unwrap();

        // Withdraw liquidity with min_out one more than expected_out. Should fail.
        let unwrap = Unwrap::Err("but expected");
        let min_out: AssetList = expected_out
            .to_vec()
            .into_iter()
            .map(|mut a| {
                a.amount += Uint128::one();
                a
            })
            .collect::<Vec<_>>()
            .into();
        let withdraw_msg = ExecuteMsg::WithdrawLiquidity {
            amount: contract_lp_token_balance,
            min_out,
        };
        unwrap.unwrap(runner.execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[withdraw_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
            admin,
        ));

        // Withdraw liquidity with expected_out as min_out. Should succeed.
        let withdraw_msg = ExecuteMsg::WithdrawLiquidity {
            amount: contract_lp_token_balance,
            min_out: expected_out.clone(),
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
            let expected_balance = expected_out.find(&asset.info).unwrap().amount;
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
    #[test_case(PairType::Xyk {}, vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "stake_and_unstake: xyk native-native")]
    #[test_case(PairType::Stable {}, vec![("uluna",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: stableswap native-cw20")]
    #[test_case(PairType::Stable {}, vec![("apollo",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: stableswap cw20-cw20")]
    #[test_case(PairType::Stable {}, vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "stake_and_unstake: stableswap native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: concentrated native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("apollo",1_000_000), ("astro", 1_000_000)]; "stake_and_unstake: concentrated cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()), vec![("uluna",1_000_000), ("uatom", 1_000_000)]; "stake_and_unstake: concentrated native-native")]
    fn test_stake_and_unstake(
        pool_type: PairType,
        initial_liquidity: Vec<(&str, u64)>,
    ) -> RunnerResult<()> {
        let owned_runner = get_test_runner();
        let runner = owned_runner.as_ref();
        let (accs, lp_token_addr, _pair_addr, contract_addr, _asset_list) =
            setup_pool_and_contract(&runner, pool_type, initial_liquidity).unwrap();

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
            stake_all_lp_tokens(&runner, contract_addr.clone(), lp_token_addr.clone(), admin)
                .events;

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
    #[test_case(PairType::Stable { },vec![("uluna",1_000_000), ("uatom", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: stable swap pool, native-native")]
    #[test_case(PairType::Stable { },vec![("uluna",1_000_000), ("uatom", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: stable swap pool, high slippage, native-native")]
    #[test_case(PairType::Stable { },vec![("uluna",68_582_147), ("uatom", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: stable swap pool, random prices, native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",1_000_000), ("astro", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: concentrated pool, high slippage, native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",68_582_147), ("astro", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, random prices, native-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("apollo",1_000_000), ("astro", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("apollo",1_000_000), ("astro", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: concentrated pool, high slippage, cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("apollo",68_582_147), ("astro", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, random prices, cw20-cw20")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",1_000_000), ("uatom", 1_000_000)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",1_000_000), ("uatom", 1_000_000)], Uint128::new(100_000_000); "swap_and_simulate_swap: concentrated pool, high slippage, native-native")]
    #[test_case(PairType::Custom("concentrated".to_string()),vec![("uluna",68_582_147), ("uatom", 3_467_256)], Uint128::new(1_000_000); "swap_and_simulate_swap: concentrated pool, random prices, native-native")]
    fn test_swap_and_simulate_swap(
        pool_type: PairType,
        initial_liquidity: Vec<(&str, u64)>,
        amount: Uint128,
    ) {
        let owned_runner = get_test_runner();
        let runner = owned_runner.as_ref();
        let (accs, _lp_token_addr, _pair_addr, contract_addr, asset_list) =
            setup_pool_and_contract(&runner, pool_type, initial_liquidity).unwrap();

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

    #[test]
    fn test_get_pool_for_lp_token() {
        let owned_runner = get_test_runner();
        let runner = owned_runner.as_ref();
        let (_accs, lp_token_addr, pair_addr, contract_addr, asset_list) = setup_pool_and_contract(
            &runner,
            PairType::Xyk {},
            vec![("uluna", 1_000_000), ("uatom", 1_000_000)],
        )
        .unwrap();

        let wasm = Wasm::new(&runner);

        let query = QueryMsg::GetPoolForLpToken {
            lp_token: AssetInfo::Cw20(Addr::unchecked(lp_token_addr.clone())),
        };
        let pool = wasm.query::<_, Pool>(&contract_addr, &query).unwrap();

        match pool {
            Pool::Astroport(pool) => {
                assert_eq!(pool.lp_token_addr, Addr::unchecked(lp_token_addr));
                assert_eq!(pool.pair_addr, Addr::unchecked(pair_addr));
                assert_eq!(
                    pool.pool_assets,
                    asset_list
                        .into_iter()
                        .map(|x| x.info.clone())
                        .collect::<Vec<AssetInfo>>()
                );
            }
            _ => panic!("Wrong pool type"),
        }
    }
}
