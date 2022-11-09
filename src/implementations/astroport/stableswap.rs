use std::cmp::Ordering;

use astroport_core::factory::PairType;
use astroport_core::querier::{query_supply, query_token_precision};
use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_slice, Addr, Decimal, Env, QuerierWrapper, Response, StdError, StdResult};
use cosmwasm_std::{Deps, Uint128};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use delegate::delegate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::traits::Pool;
use crate::CwDexError;
use astroport_core::asset::AssetInfo as AstroAssetInfo;
use astroport_core::asset::{Asset as AstroAsset, PairInfo};

use super::base_pool::AstroportBasePool;
use super::helpers::{checked_u8_mul, AstroAssetList};

pub(crate) const N_COINS: u8 = 2;
pub const AMP_PRECISION: u64 = 100;
const ITERATIONS: u8 = 32;

#[cw_serde]
pub struct AstroportStableSwapPool(AstroportBasePool);

impl AstroportStableSwapPool {
    pub fn new(deps: Deps, pair_addr: Addr) -> StdResult<Self> {
        AstroportBasePool::new(deps, pair_addr, PairType::Stable {}).map(Self)
    }
}

impl Pool for AstroportStableSwapPool {
    // Delegate all methods except `simulate_provide_liquidity` to the AstroportBasePool implementations
    delegate!(
        to self.0 {
            fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError>;
            fn simulate_withdraw_liquidity(&self, deps: Deps, asset: Asset) -> Result<AssetList, CwDexError>;
            fn simulate_swap(&self, deps: Deps, offer_asset: Asset, ask_asset_info: AssetInfo, sender: Option<String>) -> StdResult<Uint128>;
            fn provide_liquidity(&self, deps: Deps, env: &Env, assets: AssetList, slippage_tolerance: Option<Decimal>) -> Result<Response, CwDexError>;
            fn withdraw_liquidity(&self, deps: Deps, env: &Env, asset: Asset) -> Result<Response, CwDexError>;
            fn swap(&self, deps: Deps, env: &Env, offer_asset: Asset, ask_asset_info: AssetInfo, minimum_out_amount: Uint128,) -> Result<Response, CwDexError>;
        }
    );

    // Math for providing liquidity to the pool. This logic is copied from the astroport implementation here:
    // https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L338-L501
    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let assets: AstroAssetList = assets.to_owned().try_into()?;
        let config =
            query_pair_config(&deps.querier, &Addr::unchecked(self.0.pair_addr.to_string()))?;
        let mut pools: [AstroAsset; 2] =
            config.pair_info.query_pools(&deps.querier, self.0.pair_addr.to_owned())?;
        let deposits: [Uint128; 2] = [
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[0].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[1].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() && deposits[1].is_zero() {
            return Err(CwDexError::InvalidZeroAmount {});
        }

        for (i, pool) in pools.iter_mut().enumerate() {
            // we cannot put a zero amount into an empty pool.
            if deposits[i].is_zero() && pool.amount.is_zero() {
                return Err(CwDexError::InvalidProvideLPsWithSingleToken {});
            }
        }

        let token_precision_0 = query_token_precision(&deps.querier, pools[0].info.clone())?;
        let token_precision_1 = query_token_precision(&deps.querier, pools[1].info.clone())?;

        let greater_precision = token_precision_0.max(token_precision_1);

        let deposit_amount_0 = adjust_precision(deposits[0], token_precision_0, greater_precision)?;
        let deposit_amount_1 = adjust_precision(deposits[1], token_precision_1, greater_precision)?;

        let total_share = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;
        let share = if total_share.is_zero() {
            let liquidity_token_precision = query_token_precision(
                &deps.querier,
                AstroAssetInfo::Token {
                    contract_addr: config.pair_info.liquidity_token.clone(),
                },
            )?;

            // Initial share = collateral amount
            adjust_precision(
                Uint128::new(
                    (U256::from(deposit_amount_0.u128()) * U256::from(deposit_amount_1.u128()))
                        .integer_sqrt()
                        .as_u128(),
                ),
                greater_precision,
                liquidity_token_precision,
            )?
        } else {
            let leverage =
                compute_current_amp(&config, &env)?.checked_mul(u64::from(N_COINS)).unwrap();

            let mut pool_amount_0 =
                adjust_precision(pools[0].amount, token_precision_0, greater_precision)?;
            let mut pool_amount_1 =
                adjust_precision(pools[1].amount, token_precision_1, greater_precision)?;

            let d_before_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            pool_amount_0 = pool_amount_0.checked_add(deposit_amount_0)?;
            pool_amount_1 = pool_amount_1.checked_add(deposit_amount_1)?;

            let d_after_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            // d after adding liquidity may be less than or equal to d before adding liquidity because of rounding
            if d_before_addition_liquidity >= d_after_addition_liquidity {
                return Err(CwDexError::LiquidityAmountTooSmall {});
            }

            total_share.multiply_ratio(
                d_after_addition_liquidity - d_before_addition_liquidity,
                d_before_addition_liquidity,
            )
        };

        if share.is_zero() {
            return Err(CwDexError::Std(StdError::generic_err("Insufficient amount of liquidity")));
        }

        let lp_token = Asset {
            info: AssetInfoBase::Cw20(Addr::unchecked(self.0.lp_token_addr.to_string())),
            amount: share,
        };
        Ok(lp_token)
    }
}

/// ## Description
/// Compute the current pool amplification coefficient (AMP).
/// ## Params
/// * **config** is an object of type [`Config`].
///
/// * **env** is an object of type [`Env`].
///
/// This function is needed to calculate how many LP shares a user should get when providing liquidity but is
/// not publicly exposed in the package. Copied from the astro implementation here:
/// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L1492-L1515
fn compute_current_amp(config: &Config, env: &Env) -> StdResult<u64> {
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
fn adjust_precision(
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

/// ## Description
/// Computes stable swap invariant (D)
///
/// * **Equation**
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// ## Params
/// * **leverage** is the object of type [`u128`].
///
/// * **amount_a** is the object of type [`u128`].
///
/// * **amount_b** is the object of type [`u128`].
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
            d_product = d_product.checked_mul(d)?.checked_div(amount_a_times_coins)?;
            d_product = d_product.checked_mul(d)?.checked_div(amount_b_times_coins)?;
            d_previous = d;
            //d = (leverage * sum_x + d_p * n_coins) * d / ((leverage - 1) * d + (n_coins + 1) * d_p);
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
/// Calculates step
///
/// * **Equation**:
///
/// d = (leverage * sum_x + d_product * n_coins) * initial_d / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
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

// Astroport StableSwap pair does not return needed Config elements with smart query
// Raw query gets all the necessary elements
pub fn query_pair_config(querier: &QuerierWrapper, pair: &Addr) -> StdResult<Config> {
    if let Some(res) = querier.query_wasm_raw(pair, b"config".as_slice())? {
        let res: Config = from_slice(&res)?;
        Ok(res)
    } else {
        Err(cosmwasm_std::StdError::GenericErr {
            msg: "Raw query failed: config not found on pair address".to_string(),
        })
    }
}

/// ## Description
/// This structure describes the main control config of pair stable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the type of pair info available in [`PairInfo`]
    pub pair_info: PairInfo,
    /// the factory contract address
    pub factory_addr: Addr,
    /// The last time block
    pub block_time_last: u64,
    /// The last cumulative price 0 asset in pool
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price 1 asset in pool
    pub price1_cumulative_last: Uint128,
    pub init_amp: u64,
    pub init_amp_time: u64,
    pub next_amp: u64,
    pub next_amp_time: u64,
}
