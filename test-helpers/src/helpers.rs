use apollo_utils::assets::separate_natives_and_cw20s;
use cosmwasm_std::Uint128;
use cw_asset::AssetList;
use osmosis_testing::cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse;
use osmosis_testing::{Module, Runner, RunnerResult, SigningAccount, Wasm};

use cw_dex_test_contract::msg::{ExecuteMsg, InstantiateMsg};

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

pub fn provide_liquidity<'a, R: Runner<'a>>(
    runner: &'a R,
    contract_addr: String,
    assets: AssetList,
    min_out: Uint128,
    signer: &SigningAccount,
) {
    let (funds, _) = separate_natives_and_cw20s(&assets);

    // TODO: Increase allowance for cw20 assets

    // Provide liquidity
    let provide_msg = ExecuteMsg::ProvideLiquidity { assets, min_out };
    runner
        .execute_cosmos_msgs::<MsgExecuteContractResponse>(
            &[provide_msg.into_cosmos_msg(contract_addr, funds)],
            signer,
        )
        .unwrap();
}
