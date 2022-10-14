use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use cosmwasm_std::{StdError, StdResult};
use cw_asset::{AssetInfo, AssetList};

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
                .map(|a| {
                    Ok(AstroAsset {
                        info: match &a.info {
                            AssetInfo::Native(denom) => Ok(AstroAssetInfo::NativeToken {
                                denom: denom.to_string(),
                            }),
                            AssetInfo::Cw20(contract_addr) => Ok(AstroAssetInfo::Token {
                                contract_addr: contract_addr.clone(),
                            }),
                            _ => Err(StdError::generic_err("Invalid asset info")),
                        }?,
                        amount: a.amount,
                    })
                })
                .collect::<StdResult<Vec<AstroAsset>>>()?,
        ))
    }
}
