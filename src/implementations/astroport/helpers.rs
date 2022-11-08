use std::cmp::Ordering;

use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use cosmwasm_std::{Env, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

use super::querier::Config;

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
/// Compute the current pool amplification coefficient (AMP).
/// ## Params
/// * **config** is an object of type [`Config`].
///
/// * **env** is an object of type [`Env`].
///
/// Copied from the astro implementation here:
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1492-L1515
pub fn compute_current_amp(config: &Config, env: &Env) -> StdResult<u64> {
    let block_time = env.block.time.seconds();

    if block_time < config.next_amp_time {
        let elapsed_time =
            Uint128::from(block_time).checked_sub(Uint128::from(config.init_amp_time))?;
        let time_range =
            Uint128::from(config.next_amp_time).checked_sub(Uint128::from(config.init_amp_time))?;
        let init_amp = Uint128::from(config.init_amp);
        let next_amp = Uint128::from(config.next_amp);

        if config.next_amp > config.init_amp {
            let amp_range = next_amp - init_amp;
            let res = init_amp + (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.u128() as u64)
        } else {
            let amp_range = init_amp - next_amp;
            let res = init_amp - (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.u128() as u64)
        }
    } else {
        Ok(config.next_amp)
    }
}

/// ## Description
/// Return a value using a newly specified precision.
/// ## Params
/// * **value** is an object of type [`Uint128`]. This is the value that will have its precision adjusted.
///
/// * **current_precision** is an object of type [`u8`]. This is the `value`'s current precision
///
/// * **new_precision** is an object of type [`u8`]. This is the new precision to use when returning the `value`.
///
/// Copied from the astro code here:
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1269
pub(crate) fn adjust_precision(
    value: Uint128,
    current_precision: u8,
    new_precision: u8,
) -> StdResult<Uint128> {
    Ok(match current_precision.cmp(&new_precision) {
        Ordering::Equal => value,
        Ordering::Less => value
            .checked_mul(Uint128::new(10_u128.pow((new_precision - current_precision) as u32)))?,
        Ordering::Greater => value
            .checked_div(Uint128::new(10_u128.pow((current_precision - new_precision) as u32)))?,
    })
}
