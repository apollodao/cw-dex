mod tests {
    use apollo_cw_asset::{Asset, AssetInfo};
    use apollo_utils::coins::coin_from_str;
    use apollo_utils::submessages::{find_event, parse_attribute_value};
    use cosmwasm_std::{Coin, SubMsgResponse, Uint128};
    use cw_dex_test_contract::msg::{ExecuteMsg, QueryMsg};
    use cw_dex_test_helpers::osmosis::setup_pool_and_test_contract;
    use cw_dex_test_helpers::provide_liquidity;
    use cw_it::helpers::{bank_balance_query, bank_send};
    use cw_it::osmosis::{OsmosisPoolType, OsmosisTestPool};
    use osmosis_test_tube::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
    use osmosis_test_tube::{
        Account, ExecuteResponse, Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount,
        Wasm,
    };

    use test_case::test_case;

    const DENOM0: &str = "denom0";
    const DENOM1: &str = "denom1";

    const TWO_WEEKS_IN_SECS: u64 = 1_209_600;

    const ONE_MILLION: Uint128 = Uint128::new(1_000_000);

    // One hundred trillion is the LP token factor on osmosis.
    const HUNDRED_TRILLION: Uint128 = Uint128::new(100_000_000_000_000);
    const INITIAL_LIQUIDITY: Uint128 = ONE_MILLION;

    const TEST_CONTRACT_WASM_FILE_PATH: &str =
        "../target/wasm32-unknown-unknown/release/osmosis_test_contract.wasm";

    fn setup_pool_and_contract(
        test_pool: &OsmosisTestPool,
        lock_duration: Option<u64>,
    ) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
        setup_pool_and_test_contract(
            test_pool,
            lock_duration.unwrap_or(TWO_WEEKS_IN_SECS),
            1, // Lock ID. Since it is the first lock it will be 1.
            TEST_CONTRACT_WASM_FILE_PATH,
        )
    }

    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], vec![1_000_000, 1_000_000], false, Uint128::new(1_000_000) * HUNDRED_TRILLION ; "basic two pool")]
    #[test_case(OsmosisPoolType::Basic, vec![1, 1], vec![1, 1], false, HUNDRED_TRILLION ; "basic pool adding small liquidity")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], vec![1_000_000, 1_000_000], true, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "basic pool simulate min_out")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 500_000], vec![1_000_000, 500_000], true, Uint128::new(500_000) * HUNDRED_TRILLION ; "basic pool uneven assets simulate min_out")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], vec![1_000_000], false, Uint128::new(41244468918255344200) ; "basic pool single sided join")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000], vec![1_000_000], true, Uint128::new(41244468918255344200) ; "basic pool single sided with min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], vec![1_000_000, 1_000_000], false, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "stable swap pool")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], vec![1_000_000, 1_000_000], true, INITIAL_LIQUIDITY * HUNDRED_TRILLION ; "stable swap pool simulate min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, vec![1_000_000, 500_000], vec![1_000_000, 500_000], true,
                Uint128::new(500_000) * HUNDRED_TRILLION; "stable swap pool uneven assets simulate min_out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1, 1], vec![1, 1], false, HUNDRED_TRILLION ; "stable swap pool adding small liquidity")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], vec![1_000_000], false, Uint128::new(49291868209838867187) ; "stable swap pool single sided join")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] },
                vec![1_000_000, 1_000_000], vec![1_000_000], true, Uint128::new(49291868209838867187) ; "stable swap pool single sided join simulate min out")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, vec![1_000_000, 500_000], vec![1_000_000], true,
                Uint128::new(49291868209838867187); "stable swap pool uneven assets single sided join simulate min_out")]
    // TODO: Below test case fails in to execute with 1 as input amount. It works with 2.
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1] }, 
                vec![1, 1], vec![2], false, Uint128::new(50000000000000) ; "stable swap pool adding small liquidity single sided join")]
    #[test_case(OsmosisPoolType::StableSwap { scaling_factors: vec![1, 1, 1] }, vec![1_000_000, 1_000_000, 1_000_000], vec![1_000_000], true,
                Uint128::new(31745559529924392699); "stable swap tri pool single sided join simulate min_out")]
    #[test_case(OsmosisPoolType::Basic, vec![15285530166225853066258034993614457275, 232617116927201963877937669474165998946, 264289094223465729488394101802965428741, 64185674872412917334356861610378478311],
                vec![19881901777062602000000000000000000], true,
                Uint128::new(31745559529924392699); "basic quad pool single sided join simulate min_out")]
    #[test_case(OsmosisPoolType::Balancer { pool_weights: vec![2, 1] }, vec![500_000, 1_000_000], vec![500_000, 1_000_000], false,
                Uint128::new(500_000) * HUNDRED_TRILLION ; "balancer pool 2:1 weigths")]
    #[test_case(OsmosisPoolType::Basic, vec![1_000_000, 1_000_000, 1_000_000], vec![1_000_000, 1_000_000, 1_000_000], false, Uint128::new(1_000_000) * HUNDRED_TRILLION ; "even tri pool")]
    #[test_case(OsmosisPoolType::Basic, vec![1, 1, 1], vec![1, 1, 1], false, HUNDRED_TRILLION ; "even tri pool small liquidity")]
    pub fn test_provide_liquidity(
        pool_type: OsmosisPoolType,
        initial_liquidity: Vec<u128>,
        added_liquidity: Vec<u128>,
        min_out: bool,
        expected_lps: Uint128,
    ) {
        let initial_liquidity = initial_liquidity
            .iter()
            .enumerate()
            .map(|(i, amount)| Coin::new(1_000_000, format!("denom{}", i)))
            .collect();
        let test_pool = OsmosisTestPool::new(initial_liquidity, pool_type);
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(&test_pool, None).unwrap();
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
        let initial_liquidity = initial_liquidity
            .iter()
            .enumerate()
            .map(|(i, amount)| Coin::new(*amount as u128, format!("denom{}", i)))
            .collect();

        let test_pool = OsmosisTestPool::new(initial_liquidity, pool_type);
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(&test_pool, None).unwrap();
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
        let test_pool = OsmosisTestPool::new(
            vec![
                Coin::new(1_000_000, "denom0"),
                Coin::new(1_000_000, "denom1"),
            ],
            OsmosisPoolType::Basic,
        );
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(&test_pool, Some(lock_duration))?;
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
        let test_pool = OsmosisTestPool::new(
            vec![
                Coin::new(1_000_000_000, "denom0"),
                Coin::new(1_000_000_000, "denom1"),
            ],
            OsmosisPoolType::Basic,
        );
        let (runner, accs, pool_id, contract_addr) =
            setup_pool_and_contract(&test_pool, None).unwrap();
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
            lockup_id: Some(1),
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
        let test_pool = OsmosisTestPool::new(
            vec![
                Coin::new(1_000_000_000, "denom0"),
                Coin::new(1_000_000_000, "denom1"),
            ],
            pool_type,
        );

        let (runner, accs, _, contract_addr) = setup_pool_and_contract(&test_pool, None).unwrap();

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
