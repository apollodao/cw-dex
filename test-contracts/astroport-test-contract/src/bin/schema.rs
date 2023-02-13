use cosmwasm_schema::write_api;

use cw_dex_test_contract::msg::{AstroportContractInstantiateMsg, ExecuteMsg, QueryMsg};

fn main() {
    write_api! {
        instantiate: AstroportContractInstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
