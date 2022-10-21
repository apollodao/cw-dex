use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20_0_10_3::Denom;
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::CwDexError;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JunoAssetInfo(pub(crate) Denom);

impl TryFrom<AssetInfo> for JunoAssetInfo {
    type Error = StdError;
    fn try_from(info: AssetInfo) -> StdResult<Self> {
        match info {
            AssetInfo::Native(denom) => Ok(JunoAssetInfo(Denom::Native(denom))),
            AssetInfo::Cw20(addr) => Ok(JunoAssetInfo(Denom::Cw20(addr))),
            _ => Err(StdError::generic_err("Can only convert native or Cw20 to JunoAssetInfo")),
        }
    }
}

impl From<JunoAssetInfo> for AssetInfo {
    fn from(info: JunoAssetInfo) -> Self {
        match info.0 {
            Denom::Native(denom) => AssetInfo::Native(denom),
            Denom::Cw20(addr) => AssetInfo::Cw20(addr),
        }
    }
}

impl From<Denom> for JunoAssetInfo {
    fn from(denom: Denom) -> Self {
        JunoAssetInfo(denom)
    }
}

impl PartialEq<AssetInfo> for JunoAssetInfo {
    fn eq(&self, other: &AssetInfo) -> bool {
        match self {
            JunoAssetInfo(Denom::Native(denom)) => match other {
                AssetInfo::Native(other_denom) => denom == other_denom,
                _ => false,
            },
            JunoAssetInfo(Denom::Cw20(addr)) => match other {
                AssetInfo::Cw20(other_addr) => addr == other_addr,
                _ => false,
            },
        }
    }

    fn ne(&self, other: &AssetInfo) -> bool {
        !self.eq(other)
    }
}

#[derive(Clone, Debug)]
pub struct JunoAsset {
    pub(crate) info: JunoAssetInfo,
    pub(crate) amount: Uint128,
}

impl TryFrom<&Asset> for JunoAsset {
    type Error = StdError;
    fn try_from(asset: &Asset) -> StdResult<Self> {
        Ok(Self {
            info: asset.info.clone().try_into()?,
            amount: asset.amount,
        })
    }
}

impl From<JunoAsset> for Asset {
    fn from(asset: JunoAsset) -> Self {
        Self {
            info: asset.info.into(),
            amount: asset.amount,
        }
    }
}
pub struct JunoAssetList(pub(crate) Vec<JunoAsset>);

// TODO: generalize this to cover Astro case also
impl TryFrom<AssetList> for JunoAssetList {
    type Error = StdError;
    fn try_from(list: AssetList) -> StdResult<Self> {
        Ok(Self(list.into_iter().map(|a| a.try_into()).collect::<StdResult<Vec<_>>>()?))
    }
}

impl From<JunoAssetList> for AssetList {
    fn from(list: JunoAssetList) -> Self {
        list.0
            .into_iter()
            .map(|a| Asset {
                info: match a.info {
                    JunoAssetInfo(Denom::Native(denom)) => AssetInfo::Native(denom),
                    JunoAssetInfo(Denom::Cw20(addr)) => AssetInfo::Cw20(addr),
                },
                amount: a.amount,
            })
            .collect::<Vec<_>>()
            .into()
    }
}

impl<'a> IntoIterator for &'a JunoAssetList {
    type Item = &'a JunoAsset;
    type IntoIter = std::slice::Iter<'a, JunoAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl JunoAssetList {
    pub(crate) fn find(&self, token: JunoAssetInfo) -> StdResult<&JunoAsset> {
        self.into_iter()
            .find(|a| a.info == token)
            .ok_or(StdError::generic_err("Token not found in JunoAssetList instance"))
    }
}

// ------------------ Junoswap math ----------------------

pub(crate) fn juno_get_lp_token_amount_to_mint(
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, CwDexError> {
    if liquidity_supply == Uint128::zero() {
        Ok(token1_amount)
    } else {
        Ok(token1_amount.checked_mul(liquidity_supply)?.checked_div(token1_reserve)?)
    }
}

pub(crate) fn juno_get_token2_amount_required(
    max_token: Uint128,
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token2_reserve: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, CwDexError> {
    if liquidity_supply == Uint128::zero() {
        Ok(max_token)
    } else {
        Ok(token1_amount
            .checked_mul(token2_reserve)?
            .checked_div(token1_reserve)?
            .checked_add(Uint128::new(1))?)
    }
}
