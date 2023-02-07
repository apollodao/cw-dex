use std::str::FromStr;

use apollo_utils::assets::separate_natives_and_cw20s;
use cosmwasm_std::{StdResult, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_asset::{AssetInfo, AssetList};
use cw_dex_test_contract::msg::{AstroportContractInstantiateMsg, ExecuteMsg, InstantiateMsg};
use osmosis_testing::cosmrs::proto::cosmos::bank::v1beta1::QueryBalanceRequest;
use osmosis_testing::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
use osmosis_testing::{
    Account, Bank, Module, Runner, RunnerExecuteResult, RunnerResult, SigningAccount, Wasm,
};

pub fn instantiate_test_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pool_id: u64,
    lock_id: u64,
    lock_duration: u64,
    signer: &SigningAccount,
) -> RunnerResult<String> {
    let init_msg = InstantiateMsg {
        pool_id,
        lock_duration,
        lock_id,
    };

    let wasm = Wasm::new(runner);
    Ok(wasm
        .instantiate(code_id, &init_msg, None, None, &[], signer)?
        .data
        .address)
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

pub fn provide_liquidity<'a, R: Runner<'a>>(
    runner: &'a R,
    contract_addr: String,
    assets: AssetList,
    min_out: Uint128,
    signer: &SigningAccount,
) {
    let (funds, cw20s) = separate_natives_and_cw20s(&assets);

    // Send cw20 tokens to the contract
    for cw20 in cw20s {
        cw20_transfer(
            runner,
            cw20.address,
            contract_addr.clone(),
            cw20.amount,
            signer,
        )
        .unwrap();
    }

    // Provide liquidity and send native tokens to contract
    let provide_msg = ExecuteMsg::ProvideLiquidity { assets, min_out };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[provide_msg.into_cosmos_msg(contract_addr, funds)],
            signer,
        )
        .unwrap();
}

pub fn cw20_mint<'a, R: Runner<'a>>(
    runner: &'a R,
    cw20_addr: String,
    recipient: String,
    amount: Uint128,
    signer: &SigningAccount,
) -> RunnerExecuteResult<MsgExecuteContractResponse> {
    let wasm = Wasm::new(runner);
    wasm.execute(
        &cw20_addr,
        &Cw20ExecuteMsg::Mint { recipient, amount },
        &[],
        signer,
    )
}

pub fn cw20_transfer<'a, R: Runner<'a>>(
    runner: &'a R,
    cw20_addr: String,
    recipient: String,
    amount: Uint128,
    signer: &SigningAccount,
) -> RunnerExecuteResult<MsgExecuteContractResponse> {
    let wasm = Wasm::new(runner);
    wasm.execute(
        &cw20_addr,
        &Cw20ExecuteMsg::Transfer { recipient, amount },
        &[],
        signer,
    )
}

/// Query the balance of a cw20 token
pub fn cw20_balance_query<'a>(
    runner: &'a impl Runner<'a>,
    cw20_addr: String,
    address: String,
) -> StdResult<Uint128> {
    let res: BalanceResponse = Wasm::new(runner)
        .query(&cw20_addr, &Cw20QueryMsg::Balance { address })
        .unwrap();

    Ok(res.balance)
}

/// Query the balance of a native token
pub fn query_token_balance<'a, R>(runner: &'a R, denom: &str, address: &str) -> Uint128
where
    R: Runner<'a>,
{
    let bank = Bank::new(runner);
    let balance = bank
        .query_balance(&QueryBalanceRequest {
            address: address.to_string(),
            denom: denom.to_string(),
        })
        .unwrap()
        .balance
        .unwrap_or_default()
        .amount;
    Uint128::from_str(&balance).unwrap()
}

/// Query the balance of a cw_asset AssetInfo
pub fn query_asset_balance<'a, R>(runner: &'a R, asset_info: &AssetInfo, address: &str) -> Uint128
where
    R: Runner<'a>,
{
    match asset_info {
        AssetInfo::Native(denom) => query_token_balance(runner, denom, address),
        AssetInfo::Cw20(contract_addr) => {
            cw20_balance_query(runner, contract_addr.to_string(), address.to_string()).unwrap()
        }
    }
}

pub fn instantiate_cw20<'a>(
    runner: &'a impl Runner<'a>,
    cw20_code_id: u64,
    init_msg: &Cw20InstantiateMsg,
    signer: &SigningAccount,
) -> StdResult<String> {
    Ok(Wasm::new(runner)
        .instantiate(
            cw20_code_id,
            init_msg,
            Some(&signer.address()),
            Some("Astro Token"),
            &[],
            signer,
        )
        .unwrap()
        .data
        .address)
}
