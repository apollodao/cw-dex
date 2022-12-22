mod tests {
    use apollo_utils::coins::coin_from_str;
    use apollo_utils::submessages::{find_event, parse_attribute_value};
    use cosmwasm_std::{Coin, SubMsgResponse, Uint128};
    use cw_asset::{Asset, AssetInfo};
    use cw_dex_test_contract::msg::{ExecuteMsg, QueryMsg};
    use cw_dex_test_helpers::osmosis::{setup_pool_and_test_contract, OsmosisPoolType};
    use cw_dex_test_helpers::provide_liquidity;
    use cw_it::helpers::{bank_balance_query, bank_send};
    use osmosis_testing::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
    use osmosis_testing::{
        Account, ExecuteResponse, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount,
        Wasm,
    };

    use test_case::test_case;

    const DENOM0: &str = "denom0";
    const DENOM1: &str = "denom1";

    const INITIAL_TWO_POOL_LIQUIDITY: &[u64] = &[1_000_000_000, 1_000_000_000];

    const TWO_WEEKS_IN_SECS: u64 = 1_209_600;

    const ONE_MILLION: Uint128 = Uint128::new(1_000_000);

    // One hundred trillion is the LP token factor on osmosis.
    const HUNDRED_TRILLION: Uint128 = Uint128::new(100_000_000_000_000);
    const INITIAL_LIQUIDITY: Uint128 = ONE_MILLION;

    const TEST_CONTRACT_WASM_FILE_PATH: &str =
        "../target/wasm32-unknown-unknown/release/osmosis_test_contract.wasm";

    fn setup_pool_and_contract(
        pool_type: OsmosisPoolType,
        initial_liquidity: Vec<u64>,
        lock_duration: Option<u64>,
    ) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
        setup_pool_and_test_contract(
            pool_type,
            initial_liquidity,
            lock_duration.unwrap_or(TWO_WEEKS_IN_SECS),
            1, // Lock ID. Since it is the first lock it will be 1.
            TEST_CONTRACT_WASM_FILE_PATH,
        )
    }

    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], false, Uint128::new(1_000_000) * HUNDRED_TRILLION ; "basic pool")]
    #[test_case(OsmosisPoolType::Basic, vec![1, 1], false, HUNDRED_TRILLION ; "basic pool adding small liquidity")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], true, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "basic pool simulate min_out")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 500_000], true, Uint128::new(500_000) * HUNDRED_TRILLION ; "basic pool uneven assets simulate min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], false, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "stable swap pool")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], true, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "stable swap pool simulate min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, vec![1_000_000, 500_000], true,
                Uint128::new(500_000) * HUNDRED_TRILLION; "stable swap pool uneven assets simulate min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1, 1], false, HUNDRED_TRILLION ; "stable swap pool adding small liquidity")]
    #[test_case(OsmosisPoolType::Balancer { pool_weights: vec![2, 1] }, vec![500_000, 1_000_000], false,
                Uint128::new(500_000) * HUNDRED_TRILLION ; "balancer pool 2:1 weigths")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000, 1_000_000], false, Uint128::new(1_000_000) * HUNDRED_TRILLION ; "even tri pool")]
    #[test_case(OsmosisPoolType::Basic, vec![1, 1, 1], false, HUNDRED_TRILLION ; "even tri pool small liquidity")]
    pub fn test_provide_liquidity(
        pool_type: OsmosisPoolType,
        added_liquidity: Vec<u64>,
        min_out: bool,
        expected_lps: Uint128,
    ) {
        let initial_liquidity = added_liquidity.iter().map(|_| 1_000_000).collect();
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(pool_type, initial_liquidity, None).unwrap();
        let admin = &accs[0];

        let coins = added_liquidity
            .into_iter()
            .enumerate()
            .map(|(i, amount)| Coin {
                denom: format!("denom{}", i),
                amount: amount.into(),
            })
            .collect::<Vec<_>>();

        // Send funds to contract
        bank_send(&runner, admin, &contract_addr, coins.clone()).unwrap();

        //Simulate provide liquidity
        let min_out = if min_out {
            let wasm = Wasm::new(&runner);
            wasm.query::<_, Uint128>(
                &contract_addr,
                &QueryMsg::SimulateProvideLiquidity {
                    assets: coins.clone().into(),
                },
            )
            .unwrap()
        } else {
            Uint128::one()
        };

        // Provide liquidity
        provide_liquidity(&runner, contract_addr.clone(), coins.into(), min_out, admin);

        // Query LP token balance after
        let lp_token_after =
            bank_balance_query(&runner, contract_addr, format!("gamm/pool/{}", pool_id)).unwrap();

        assert_eq!(lp_token_after, expected_lps);
    }

    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000] ; "basic pool")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, vec![1_000_000, 1_000_000] ; "stable swap pool")]
    fn test_withdraw_liquidity(pool_type: OsmosisPoolType, initial_liquidity: Vec<u64>) {
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(pool_type, initial_liquidity, None).unwrap();
        let admin = &accs[0];
        let lp_token_denom = format!("gamm/pool/{}", pool_id);

        //Query admin LP token balance
        let admin_lp_token_balance =
            bank_balance_query(&runner, admin.address(), lp_token_denom.clone()).unwrap();

        // Send LP tokens to the test contract
        bank_send(
            &runner,
            admin,
            &contract_addr,
            vec![Coin::new(
                admin_lp_token_balance.u128(),
                lp_token_denom.clone(),
            )],
        )
        .unwrap();

        // Withdraw liquidity. We are not allowed to withdraw all liquidity on osmosis.
        let withdraw_msg = ExecuteMsg::WithdrawLiquidity {
            amount: admin_lp_token_balance.checked_sub(Uint128::one()).unwrap(),
        };
        runner
            .execute_cosmos_msgs::<MsgExecuteContractResponse>(
                &[withdraw_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
                admin,
            )
            .unwrap();

        // Query LP token balance after
        let lp_token_balance_after =
            bank_balance_query(&runner, contract_addr, lp_token_denom).unwrap();

        // Assert that LP token balance is 1
        assert_eq!(lp_token_balance_after, Uint128::one());
    }

    fn stake_all_lp_tokens<'a, R: Runner<'a>>(
        runner: &'a R,
        contract_addr: String,
        pool_id: u64,
        signer: &SigningAccount,
    ) -> ExecuteResponse<MsgExecuteContractResponse> {
        // Query LP token balance
        let lp_token_balance = bank_balance_query(
            runner,
            contract_addr.clone(),
            format!("gamm/pool/{}", pool_id),
        )
        .unwrap();

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

    #[test_case(86_400 ; "one day lock")]
    #[test_case(604_800 ; "one week lock")]
    #[test_case(1_209_600 ; "two week lock")]
    #[test_case(1 => matches Err(_) ; "invalid lock duration")]
    fn test_stake_and_unlock(lock_duration: u64) -> RunnerResult<()> {
        let (runner, accs, pool_id, contract_addr) = setup_pool_and_contract(
            OsmosisPoolType::Basic,
            vec![1_000_000, 1_000_000],
            Some(lock_duration),
        )?;
        let admin = &accs[0];

        // Query LP token balance
        let lp_token_denom = format!("gamm/pool/{}", pool_id);
        let lp_token_balance =
            bank_balance_query(&runner, admin.address(), lp_token_denom.clone()).unwrap();

        // Send LP tokens to the test contract
        bank_send(
            &runner,
            admin,
            &contract_addr,
            vec![Coin::new(lp_token_balance.u128(), lp_token_denom.clone())],
        )
        .unwrap();

        // Stake LP tokens
        let events = stake_all_lp_tokens(&runner, contract_addr.clone(), pool_id, admin).events;

        // Parse the event data
        let response = SubMsgResponse { events, data: None };
        let event = find_event(&response, "lock_tokens").unwrap();
        let lock_owner: String = parse_attribute_value(event, "owner").unwrap();
        let amount = coin_from_str(&parse_attribute_value::<String, _>(event, "amount").unwrap());

        // Assert the lock has correct owner and amount
        assert_eq!(lock_owner, contract_addr);
        assert_eq!(amount.amount, lp_token_balance);
        assert_eq!(amount.denom, lp_token_denom);

        // Query LP token balance after
        let lp_token_balance_after =
            bank_balance_query(&runner, contract_addr.to_string(), lp_token_denom.clone()).unwrap();

        // Assert that LP token balance is 0
        assert_eq!(lp_token_balance_after, Uint128::zero());

        // Unlock LP tokens
        let unlock_msg = ExecuteMsg::Unlock {
            amount: lp_token_balance,
        };
        runner
            .execute_cosmos_msgs::<MsgExecuteContractResponse>(
                &[unlock_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
                admin,
            )
            .unwrap();

        // Query LP token balance after
        let lp_token_balance_before_unlock =
            bank_balance_query(&runner, contract_addr.to_string(), lp_token_denom.clone()).unwrap();
        assert_eq!(lp_token_balance_before_unlock, Uint128::zero());

        // Increase chain time
        runner.increase_time(lock_duration + 1);

        // Query LP token balance
        let lp_token_balance_after_unlock =
            bank_balance_query(&runner, contract_addr, lp_token_denom).unwrap();

        // Assert that LP tokens have been unlocked
        assert_eq!(lp_token_balance_after_unlock, lp_token_balance);

        Ok(())
    }

    #[test_case(false => matches Err(_) ; "not whitelisted")]
    #[test_case(true ; "whitelisted")]
    fn test_force_unlock(whitelist: bool) -> RunnerResult<()> {
        let (runner, accs, pool_id, contract_addr) = setup_pool_and_contract(
            OsmosisPoolType::Basic,
            INITIAL_TWO_POOL_LIQUIDITY.to_vec(),
            None,
        )
        .unwrap();
        let admin = &accs[0];

        // Temp variables. Fix as args
        let unlock_amount = Uint128::from(1000000u128);

        let assets = vec![
            Asset {
                info: AssetInfo::Native(DENOM0.to_string()),
                amount: ONE_MILLION,
            },
            Asset {
                info: AssetInfo::Native(DENOM1.to_string()),
                amount: ONE_MILLION,
            },
        ];

        // Provide liquidity
        provide_liquidity(
            &runner,
            contract_addr.clone(),
            assets.into(),
            Uint128::one(),
            admin,
        );

        // Stake LP tokens
        stake_all_lp_tokens(&runner, contract_addr.clone(), pool_id, admin);

        // Whitlist contract_addr
        if whitelist {
            runner.whitelist_address_for_force_unlock(&contract_addr);
        }

        // Force unlock LP tokens
        let force_unlock_msg = ExecuteMsg::ForceUnlock {
            amount: unlock_amount,
            lockup_id: 1,
        };
        runner.execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[force_unlock_msg.into_cosmos_msg(contract_addr.clone(), vec![])],
            admin,
        )?;

        // Query LP token balance
        let lp_token_balance =
            bank_balance_query(&runner, contract_addr, format!("gamm/pool/{}", pool_id)).unwrap();

        // Assert that LP tokens have been unlocked
        assert_eq!(lp_token_balance, unlock_amount);

        Ok(())
    }

    // TODO: For some reason it fails when swap amount is 1. Need to investigate
    #[test_case(OsmosisPoolType::Basic, Uint128::new(1_000_000), false ; "basic pool")]
    #[test_case(OsmosisPoolType::Basic, Uint128::new(2), false ; "basic pool small amount")]
    #[test_case(OsmosisPoolType::Basic, Uint128::new(1000000), true ; "basic pool with min out")]
    #[test_case(OsmosisPoolType::Balancer { pool_weights: vec![2, 1] }, Uint128::new(1000000), false ; "2:1 balancer pool")]
    #[test_case(OsmosisPoolType::Balancer { pool_weights: vec![2, 1] }, Uint128::new(2), false ; "2:1 balancer pool small amount")]
    #[test_case(OsmosisPoolType::Balancer { pool_weights: vec![2, 1] }, Uint128::new(1000000), true ; "2:1 balancer pool with min out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, Uint128::new(1000000), false ; "stable swap pool")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, Uint128::new(2), false ; "stable swap pool small amount")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, Uint128::new(1000000), true ; "stable swap pool with min out")]
    fn test_swap_and_simulate_swap(pool_type: OsmosisPoolType, amount: Uint128, min_out: bool) {
        let (runner, accs, _, contract_addr) =
            setup_pool_and_contract(pool_type, INITIAL_TWO_POOL_LIQUIDITY.to_vec(), None).unwrap();

        let admin = &accs[0];

        // Simulate swap
        let offer = Asset {
            info: AssetInfo::Native(DENOM0.to_string()),
            amount,
        };
        let ask = AssetInfo::Native(DENOM1.to_string());
        let wasm = Wasm::new(&runner);
        let simulate_query = QueryMsg::SimulateSwap {
            offer: offer.clone(),
            ask: ask.clone(),
            sender: Some(contract_addr.clone()),
        };
        let expected_out = wasm.query(&contract_addr, &simulate_query).unwrap();
        let min_out = if min_out {
            expected_out
        } else {
            Uint128::one()
        };

        // Swap
        let swap_msg = ExecuteMsg::Swap {
            offer: offer.clone(),
            ask: ask.clone(),
            min_out,
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
            bank_balance_query(&runner, contract_addr.to_string(), offer.info.to_string()).unwrap();
        let ask_balance = bank_balance_query(&runner, contract_addr, ask.to_string()).unwrap();

        // Assert that OSMO and ATOM balances are correct
        assert_eq!(ask_balance, expected_out);
        assert_eq!(offer_balance, Uint128::zero());
    }
}
