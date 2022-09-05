use std::convert::TryInto;

use apollo_proto_rust::{
    osmosis::gamm::v1beta1::{PoolParams, QueryPoolParamsRequest, QueryPoolParamsResponse},
    utils::encode,
    OsmosisTypeURLs,
};
use cosmwasm_std::{
    from_binary, Coin, CustomQuery, QuerierWrapper, QueryRequest, StdError, StdResult,
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
