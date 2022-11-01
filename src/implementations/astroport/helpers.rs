use std::collections::HashMap;

use crate::CwDexError;
use astroport_core::{asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo, Decimal256Ext, DecimalAsset}, querier::{query_token_precision, query_supply, query_fee_info}};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256, Deps, Env, StdError, StdResult, Uint128, Uint64, Decimal, Uint256};
use cw_asset::{Asset, AssetInfo, AssetList};
use itertools::Itertools;

use super::querier::{query_pair_config, Config, query_asset_precision};

/// The maximum number of calculation steps for Newton's method.
const ITERATIONS: u8 = 32;
pub const AMP_PRECISION: u64 = 100;

#[cw_serde]
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
pub(crate) fn compute_current_amp(_deps: Deps, env: &Env, config: Config) -> StdResult<Uint64> {
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

/// Return the amount of tokens that a specific amount of LP tokens would withdraw.
///
/// * **pools** array with assets available in the pool.
///
/// * **amount** amount of LP tokens to calculate underlying amounts for.
///
/// * **total_share** total amount of LP tokens currently issued by the pool.
pub(crate) fn get_share_in_assets(
    pools: &[AstroAsset],
    amount: Uint128,
    total_share: Uint128,
) -> Vec<AstroAsset> {
    let mut share_ratio = Decimal::zero();
    if !total_share.is_zero() {
        share_ratio = Decimal::from_ratio(amount, total_share);
    }

    pools
        .iter()
        .map(|pool| AstroAsset {
            info: pool.info.clone(),
            amount: pool.amount * share_ratio,
        })
        .collect()
}

/// Imbalanced withdraw liquidity from the pool. Returns a [`ContractError`] on failure,
/// otherwise returns the number of LP tokens to burn.
///
/// * **provided_amount** amount of provided LP tokens to withdraw liquidity with.
///
/// * **assets** specifies the assets amount to withdraw.
pub fn imbalanced_withdraw(
    deps: Deps,
    env: &Env,
    config: &Config,
    provided_amount: Uint128,
    assets: &[AstroAsset],
) -> Result<Uint128, CwDexError> {
    if assets.len() > config.pair_info.asset_infos.len() {
        return Err(CwDexError::Std(StdError::generic_err("Invalid number of assets. The Astroport supports at least 2 and at most 5 assets within a stable pool")));
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut assets_collection = assets
        .iter()
        .cloned()
        .map(|asset| {
            let precision = query_token_precision(&deps.querier, &asset.info)?;
            // Get appropriate pool
            let pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| CwDexError::Std(StdError::generic_err("The asset does not belong to the pair")))?;

            Ok((
                asset.to_decimal_asset(precision)?,
                Decimal256::with_precision(pool, precision)?,
            ))
        })
        .collect::<Result<Vec<_>, CwDexError>>()?;

    // If some assets are omitted then add them explicitly with 0 withdraw amount
    pools
        .into_iter()
        .try_for_each(|(pool_info, pool_amount)| -> StdResult<()> {
            if !assets.iter().any(|asset| asset.info == pool_info) {
                let precision = query_token_precision(&deps.querier, &pool_info)?;

                assets_collection.push((
                    DecimalAsset {
                        amount: Decimal256::zero(),
                        info: pool_info,
                    },
                    Decimal256::with_precision(pool_amount, precision)?,
                ));
            }
            Ok(())
        })?;

    let n_coins = config.pair_info.asset_infos.len() as u8;

    let amp = compute_current_amp(deps, env, config.to_owned())?;

    // Initial invariant (D)
    let old_balances = assets_collection
        .iter()
        .map(|(_, pool)| *pool)
        .collect_vec();
    let init_d = compute_d(amp, &old_balances, config.greatest_precision)?;

    // Invariant (D) after assets withdrawn
    let mut new_balances = assets_collection
        .iter()
        .cloned()
        .map(|(withdraw, pool)| Ok(pool - withdraw.amount))
        .collect::<StdResult<Vec<Decimal256>>>()?;
    let withdraw_d = compute_d(amp, &new_balances, config.greatest_precision)?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
    let fee = fee_info
        .total_fee_rate
        .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

    let fee = Decimal256::new(fee.atomics().into());

    for i in 0..n_coins as usize {
        let ideal_balance = withdraw_d.checked_multiply_ratio(old_balances[i], init_d)?;
        let difference = if ideal_balance > new_balances[i] {
            ideal_balance - new_balances[i]
        } else {
            new_balances[i] - ideal_balance
        };
        new_balances[i] -= fee.checked_mul(difference)?;
    }

    let after_fee_d = compute_d(amp, &new_balances, config.greatest_precision)?;

    let total_share = Uint256::from(query_supply(
        &deps.querier,
        &config.pair_info.liquidity_token,
    )?);
    // How many tokens do we need to burn to withdraw asked assets?
    let burn_amount = total_share
        .checked_multiply_ratio(
            init_d.atomics().checked_sub(after_fee_d.atomics())?,
            init_d.atomics(),
        )?
        .checked_add(Uint256::from(1u8))?; // In case of rounding errors - make it unfavorable for the "attacker"

    let burn_amount = burn_amount.try_into()?;

    if burn_amount > provided_amount {
        return Err(StdError::generic_err(format!(
            "Not enough LP tokens. You need {} LP tokens.",
            burn_amount
        ))
        .into());
    }

    Ok(burn_amount)
}
