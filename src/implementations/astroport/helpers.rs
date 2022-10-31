use crate::CwDexError;
use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo, Decimal256Ext};
use cosmwasm_std::{Addr, Decimal256, Deps, Env, StdError, StdResult, Uint128, Uint64};
use cw_asset::{Asset, AssetInfo, AssetList};

use super::querier::{query_pair_config, Config};

/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 32;
pub const AMP_PRECISION: u64 = 100;

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
                .map(|a| cw_asset_to_astro_asset(a))
                .collect::<StdResult<Vec<AstroAsset>>>()?,
        ))
    }
}

impl From<AstroAssetList> for AssetList {
    fn from(list: AstroAssetList) -> Self {
        list.0
            .into_iter()
            .map(|a| cw_asset::Asset {
                info: match a.info {
                    AstroAssetInfo::NativeToken {
                        denom,
                    } => AssetInfo::Native(denom),
                    AstroAssetInfo::Token {
                        contract_addr,
                    } => AssetInfo::Cw20(contract_addr),
                },
                amount: a.amount,
            })
            .collect::<Vec<Asset>>()
            .into()
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
        } => AssetInfo::Native(denom.to_string()),
        AstroAssetInfo::Token {
            contract_addr,
        } => AssetInfo::cw20(contract_addr.to_owned()),
    }
}

/// Compute the current pool amplification coefficient (AMP).
pub(crate) fn compute_current_amp(deps: Deps, env: &Env, config: Config) -> StdResult<Uint64> {
    let block_time = env.block.time.seconds();
    if block_time < config.next_amp_time {
        let elapsed_time: Uint128 = block_time.saturating_sub(config.init_amp_time).into();
        let time_range = config.next_amp_time.saturating_sub(config.init_amp_time).into();
        let init_amp = Uint128::from(config.init_amp);
        let next_amp = Uint128::from(config.next_amp);

        if next_amp > init_amp {
            let amp_range = next_amp - init_amp;
            let res = init_amp + (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.try_into()?)
        } else {
            let amp_range = init_amp - next_amp;
            let res = init_amp - (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.try_into()?)
        }
    } else {
        Ok(Uint64::from(config.next_amp))
    }
}

/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
pub(crate) fn compute_d(
    amp: Uint64,
    pools: &[Decimal256],
    greatest_precision: u8,
) -> StdResult<Decimal256> {
    if pools.iter().any(|pool| pool.is_zero()) {
        return Ok(Decimal256::zero());
    }
    let sum_x = pools.iter().fold(Decimal256::zero(), |acc, x| acc + (*x));

    if sum_x.is_zero() {
        Ok(Decimal256::zero())
    } else {
        let n_coins = pools.len() as u8;
        let ann = Decimal256::from_ratio(amp.checked_mul(n_coins.into())?.u64(), AMP_PRECISION);
        let n_coins = Decimal256::from_integer(n_coins);
        let mut d = sum_x;
        let ann_sum_x = ann * sum_x;
        for _ in 0..ITERATIONS {
            // loop: D_P = D_P * D / (_x * N_COINS)
            let d_p = pools.iter().try_fold::<_, _, StdResult<_>>(d, |acc, pool| {
                let denominator = pool.checked_mul(n_coins)?;
                acc.checked_multiply_ratio(d, denominator)
            })?;
            let d_prev = d;
            d = (ann_sum_x + d_p * n_coins) * d
                / ((ann - Decimal256::one()) * d + (n_coins + Decimal256::one()) * d_p);
            if d >= d_prev {
                if d - d_prev <= Decimal256::with_precision(1u8, greatest_precision)? {
                    return Ok(d);
                }
            } else if d < d_prev
                && d_prev - d <= Decimal256::with_precision(1u8, greatest_precision)?
            {
                return Ok(d);
            }
        }

        Ok(d)
    }
}
