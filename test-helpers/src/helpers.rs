use apollo_cw_asset::{AssetInfo, AssetList};
use apollo_utils::assets::separate_natives_and_cw20s;
use cosmwasm_std::{StdResult, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw_dex_test_contract::msg::ExecuteMsg;
use cw_it::helpers::bank_balance_query;
use cw_it::osmosis_std::types::cosmwasm::wasm::v1::MsgExecuteContractResponse;
use cw_it::test_tube::{Account, Module, Runner, RunnerExecuteResult, SigningAccount, Wasm};

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

/// Query the balance of a cw_asset AssetInfo
pub fn query_asset_balance<'a, R>(runner: &'a R, asset_info: &AssetInfo, address: &str) -> Uint128
where
    R: Runner<'a>,
{
    match asset_info {
        AssetInfo::Native(denom) => {
            bank_balance_query(runner, address.to_string(), denom.clone()).unwrap()
        }
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
