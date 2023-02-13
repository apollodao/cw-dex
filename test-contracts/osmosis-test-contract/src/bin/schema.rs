use cosmwasm_schema::write_api;

use cw_dex_test_contract::msg::{ExecuteMsg, OsmosisTestContractInstantiateMsg, QueryMsg};

fn main() {
    write_api! {
        instantiate: OsmosisTestContractInstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
