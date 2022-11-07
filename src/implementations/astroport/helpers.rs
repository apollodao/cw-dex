use astroport_core::{asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo}, U256};
use cosmwasm_std::{StdError, StdResult, Env, Uint128};
use cw_asset::{Asset, AssetInfo, AssetList};

use super::{querier::Config, N_COINS};

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

/// ## Description
/// Compute the current pool amplification coefficient (AMP).
/// ## Params
/// * **config** is an object of type [`Config`].
///
/// * **env** is an object of type [`Env`].
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
/// Computes the stableswap invariant (D).
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// ## Params
/// * **leverage** is an object of type [`u128`].
///
/// * **amount_a** is an object of type [`u128`].
///
/// * **amount_b** is an object of type [`u128`].
pub fn compute_d(leverage: u64, amount_a: u128, amount_b: u128) -> Option<u128> {
    let amount_a_times_coins =
        checked_u8_mul(&U256::from(amount_a), N_COINS)?.checked_add(U256::one())?;
    let amount_b_times_coins =
        checked_u8_mul(&U256::from(amount_b), N_COINS)?.checked_add(U256::one())?;
    let sum_x = amount_a.checked_add(amount_b)?; // sum(x_i), a.k.a S
    if sum_x == 0 {
        Some(0)
    } else {
        let mut d_previous: U256;
        let mut d: U256 = sum_x.into();

        // Newton's method to approximate D
        for _ in 0..ITERATIONS {
            let mut d_product = d;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_a_times_coins)?;
            d_product = d_product
                .checked_mul(d)?
                .checked_div(amount_b_times_coins)?;
            d_previous = d;
            // d = (leverage * sum_x + d_p * n_coins) * d / ((leverage - 1) * d + (n_coins + 1) * d_p);
            d = calculate_step(&d, leverage, sum_x, &d_product)?;
            // Equality with the precision of 1
            if d == d_previous {
                break;
            }
        }
        u128::try_from(d).ok()
    }
}

/// ## Description
/// Returns self multiplied by b.
fn checked_u8_mul(a: &U256, b: u8) -> Option<U256> {
    let mut result = *a;
    for _ in 1..b {
        result = result.checked_add(*a)?;
    }
    Some(result)
}

/// ## Description
/// Helper function used to calculate the D invariant as a last step in the `compute_d` public function.
///
/// * **Equation**:
///
/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/math.rs#L119
fn calculate_step(initial_d: &U256, leverage: u64, sum_x: u128, d_product: &U256) -> Option<U256> {
    let leverage_mul = U256::from(leverage).checked_mul(sum_x.into())? / AMP_PRECISION;
    let d_p_mul = checked_u8_mul(d_product, N_COINS)?;

    let l_val = leverage_mul.checked_add(d_p_mul)?.checked_mul(*initial_d)?;

    let leverage_sub =
        initial_d.checked_mul((leverage.checked_sub(AMP_PRECISION)?).into())? / AMP_PRECISION;
    let n_coins_sum = checked_u8_mul(d_product, N_COINS.checked_add(1)?)?;

    let r_val = leverage_sub.checked_add(n_coins_sum)?;

    l_val.checked_div(r_val)
}

