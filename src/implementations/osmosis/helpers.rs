use std::{convert::TryInto, time::Duration};

use cosmwasm_std::{
    from_binary, Coin, CustomQuery, QuerierWrapper, QueryRequest, StdError, StdResult,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use osmosis_std::types::osmosis::gamm::v1beta1::{
    GammQuerier, PoolParams, QueryPoolParamsRequest, QueryPoolParamsResponse,
};

use crate::error::CwDexError;

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

pub(crate) fn assert_native_asset_info(asset_info: &AssetInfo) -> Result<String, CwDexError> {
    match asset_info {
        cw_asset::AssetInfoBase::Native(denom) => Ok(denom.clone()),
        _ => Err(CwDexError::InvalidOutAsset {}),
    }
}

pub(crate) fn merge_assets<'a, A: Into<&'a AssetList>>(assets: A) -> StdResult<AssetList> {
    let asset_list = assets.into();
    let mut merged = AssetList::new();
    for asset in asset_list {
        merged.add(asset)?;
    }
    Ok(merged)
}
