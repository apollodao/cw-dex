// use cosmwasm_std::testing::MockQuerier;
// use cosmwasm_std::{
//     from_binary, from_slice, Addr, Coin, Decimal, Empty, Querier, QuerierResult, QueryRequest,
//     StdResult, SystemError, WasmQuery,
// };
// use cw20::Cw20QueryMsg;

// use crate::unittest::cw20_querier::CustomRawQueryMsg;

// use super::cw20_querier::{Cw20Querier, RawQuerier};

// pub struct CustomMockQuerier {
//     base: MockQuerier<Empty>,
//     cw20_querier: Cw20Querier,
//     raw_querier: RawQuerier,
// }

// impl Default for CustomMockQuerier {
//     fn default() -> Self {
//         CustomMockQuerier {
//             base: MockQuerier::<Empty>::new(&[]),
//             cw20_querier: Cw20Querier::default(),
//             raw_querier: RawQuerier::default(),
//         }
//     }
// }

// impl Querier for CustomMockQuerier {
//     fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
//         let request: QueryRequest<Empty> = match from_slice(bin_request) {
//             Ok(v) => v,
//             Err(e) => {
//                 return Err(SystemError::InvalidRequest {
//                     error: format!("[mock]: failed to parse query request {}", e),
//                     request: bin_request.into(),
//                 })
//                 .into()
//             }
//         };
//         self.handle_query(&request)
//     }
// }

// impl CustomMockQuerier {
//     pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
//         match request {
//             QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
//                 let contract_addr = Addr::unchecked(contract_addr);

//                 let parse_cw20_query: StdResult<Cw20QueryMsg> = from_binary(msg);
//                 if let Ok(cw20_query) = parse_cw20_query {
//                     return self
//                         .cw20_querier
//                         .handle_smart_query(&contract_addr, cw20_query);
//                 }

//                 panic!("[mock]: unsupported wasm query {:?}", msg);
//             }

//             QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
//                 // For now we only support raw factory fee
//                 // TODO: Find a way to get the "key" of the map from Binary<Path>
//                 if contract_addr.contains("factory") {
//                     let factory_msg = CustomRawQueryMsg::Factory {
//                         key: "fee".to_string(),
//                     };

//                     return self.raw_querier.handle_raw_query(factory_msg);
//                 }
//                 panic!("[mock]: unsupported wasm query {:?}", key);
//             }

//             _ => self.base.handle_query(request),
//         }
//     }

//     pub fn set_base_balances(&mut self, address: &str, balances: &[Coin]) {
//         self.base.update_balance(address, balances.to_vec());
//     }

//     pub fn set_cw20_balance(&mut self, contract: &str, user: &str, balance: u128) {
//         self.cw20_querier.set_balance(contract, user, balance);
//     }

//     pub fn set_raw_factory_fee(&mut self, fee: Decimal) {
//         self.raw_querier.set_fee(fee);
//     }
// }
