use osmosis_testing::{Module, Runner, SigningAccount, Wasm};

use crate::msg::InstantiateMsg;

pub fn instantiate_test_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pool_id: u64,
    lock_id: u64,
    lock_duration: u64,
    signer: &SigningAccount,
) -> String {
    let init_msg = InstantiateMsg {
        pool_id,
        lock_duration,
        lock_id,
    };

    let wasm = Wasm::new(runner);
    wasm.instantiate(code_id, &init_msg, None, None, &[], signer)
        .unwrap()
        .data
        .address
}
