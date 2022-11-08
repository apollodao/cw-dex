use astroport_core::{asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo}, U256};
use cosmwasm_std::{StdError, StdResult};
use cw_asset::{Asset, AssetInfo, AssetList};

pub(crate) struct AstroAssetList(pub(crate) Vec<AstroAsset>);

impl From<AstroAssetList> for Vec<AstroAsset> {
    fn from(list: AstroAssetList) -> Self {
        list.0
    }
}

impl From<Vec<AstroAsset>> for AstroAssetList {
    fn from(list: Vec<AstroAsset>) -> Self {
        AstroAssetList(list)
    }
}

impl TryFrom<AssetList> for AstroAssetList {
    type Error = StdError;
    fn try_from(list: AssetList) -> StdResult<Self> {
        Ok(Self(
            list.into_iter()
                .map(cw_asset_to_astro_asset)
                .collect::<StdResult<Vec<AstroAsset>>>()?,
        ))
    }
}

impl From<AstroAssetList> for AssetList {
    fn from(list: AstroAssetList) -> Self {
        list.0.iter().map(astro_asset_to_cw_asset).collect::<Vec<Asset>>().into()
    }
}

pub(crate) fn astro_asset_to_cw_asset(astro_asset: &AstroAsset) -> Asset {
    Asset {
        info: astro_asset_info_to_cw_asset_info(&astro_asset.info),
        amount: astro_asset.amount,
    }
}

pub(crate) fn cw_asset_to_astro_asset(asset: &Asset) -> StdResult<AstroAsset> {
    Ok(AstroAsset {
        info: cw_asset_info_to_astro_asset_info(&asset.info)?,
        amount: asset.amount,
    })
}

pub(crate) fn cw_asset_info_to_astro_asset_info(
    asset_info: &AssetInfo,
) -> StdResult<AstroAssetInfo> {
    match asset_info {
        AssetInfo::Native(denom) => Ok(AstroAssetInfo::NativeToken {
            denom: denom.to_string(),
        }),
        AssetInfo::Cw20(contract_addr) => Ok(AstroAssetInfo::Token {
            contract_addr: contract_addr.clone(),
        }),
        _ => Err(StdError::generic_err("Invalid asset info")),
    }
}

pub(crate) fn astro_asset_info_to_cw_asset_info(asset_info: &AstroAssetInfo) -> AssetInfo {
    match asset_info {
        AstroAssetInfo::NativeToken {
            denom,
        } => AssetInfo::Native(denom.clone()),
        AstroAssetInfo::Token {
            contract_addr,
        } => AssetInfo::cw20(contract_addr.clone()),
    }
}

/// ## Description
/// Returns self multiplied by b
pub fn checked_u8_mul(a: &U256, b: u8) -> Option<U256> {
    let mut result = *a;
    for _ in 1..b {
        result = result.checked_add(*a)?;
    }
    Some(result)
}