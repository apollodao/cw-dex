use std::{convert::TryInto, time::Duration};

use apollo_proto_rust::{
    osmosis::{
        gamm::v1beta1::{PoolParams, QueryPoolParamsRequest, QueryPoolParamsResponse},
        lockup::{
            AccountLockedLongerDurationNotUnlockingOnlyRequest,
            AccountLockedLongerDurationNotUnlockingOnlyResponse, PeriodLock,
        },
    },
    utils::encode,
    OsmosisTypeURLs,
};
use cosmwasm_std::{
    from_binary, Addr, Coin, CustomQuery, QuerierWrapper, QueryRequest, StdError, StdResult,
};
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::CwDexError;

pub(crate) fn query_pool_params<C: CustomQuery>(
    querier: QuerierWrapper<C>,
    pool_id: u64,
) -> StdResult<PoolParams> {
    from_binary(
        &querier
            .query::<QueryPoolParamsResponse>(&QueryRequest::Stargate {
                path: OsmosisTypeURLs::QueryPoolParams {
                    pool_id,
                }
                .to_string(),
                data: encode(QueryPoolParamsRequest {
                    pool_id,
                }),
            })?
            .params
            .ok_or(StdError::generic_err("failed to query pool params"))?
            .value
            .as_slice()
            .into(),
    )
}

pub(crate) trait ToProtobufDuration {
    fn to_protobuf_duration(&self) -> apollo_proto_rust::google::protobuf::Duration;
}

impl ToProtobufDuration for Duration {
    fn to_protobuf_duration(&self) -> apollo_proto_rust::google::protobuf::Duration {
        apollo_proto_rust::google::protobuf::Duration {
            seconds: self.as_secs() as i64,
            nanos: self.subsec_nanos() as i32,
        }
    }
}

pub(crate) fn query_lock<C: CustomQuery>(
    querier: QuerierWrapper<C>,
    owner: &Addr,
    duration: Duration,
) -> StdResult<PeriodLock> {
    let locks = querier
        .query::<AccountLockedLongerDurationNotUnlockingOnlyResponse>(&QueryRequest::Stargate {
            path: OsmosisTypeURLs::QueryAccountLockedLongerDurationNotUnlockingOnly {
                owner: owner.to_string(),
            }
            .to_string(),
            data: encode(AccountLockedLongerDurationNotUnlockingOnlyRequest {
                owner: owner.to_string(),
                duration: Some(duration.to_protobuf_duration()),
            }),
        })?
        .locks;

    // Unwrap PeriodLock object from response
    // TODO: Generalize to support a user that has multiple locks in different lps or durations.
    if locks.len() == 1 {
        Ok(locks[0].clone())
    } else if locks.len() == 0 {
        Err(StdError::generic_err("osmosis error: no lock found".to_string()))
    } else {
        Err(StdError::generic_err("osmosis error: multiple locks found".to_string()))
    }
}

pub(crate) fn assert_only_native_coins(assets: AssetList) -> Result<Vec<Coin>, CwDexError> {
    assets.into_iter().map(assert_native_coin).collect::<Result<Vec<Coin>, CwDexError>>()
}

pub(crate) fn assert_native_coin(asset: &Asset) -> Result<Coin, CwDexError> {
    match asset.info {
        AssetInfo::Native(_) => asset.try_into().map_err(|e: StdError| e.into()),
        _ => Err(CwDexError::InvalidInAsset {
            a: asset.clone(),
        }),
    }
}
