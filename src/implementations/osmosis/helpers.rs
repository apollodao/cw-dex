use std::time::Duration;

use cosmwasm_std::{QuerierWrapper, StdError, StdResult};
use osmosis_std::types::osmosis::gamm::v1beta1::GammQuerier;

pub(crate) trait ToProtobufDuration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration;
}

impl ToProtobufDuration for Duration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration {
        osmosis_std::shim::Duration {
            seconds: self.as_secs() as i64,
            nanos: self.subsec_nanos() as i32,
        }
    }
}

pub(crate) fn query_lp_denom(querier: &QuerierWrapper, pool_id: u64) -> StdResult<String> {
    GammQuerier::new(querier)
        .total_shares(pool_id)?
        .total_shares
        .ok_or_else(|| StdError::generic_err("No total shares found for pool"))
        .map(|coin| coin.denom)
}
