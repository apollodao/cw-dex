use prost::DecodeError;
use std::{
    ops::{Neg, Sub},
    str::FromStr,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Decimal, QuerierWrapper, QueryRequest, StdError, StdResult, Uint128};
use num_bigint::BigInt;
use osmo_bindings::{OsmosisQuery, PoolStateResponse};
use osmosis_std::types::osmosis::gamm::v1beta1::{GammQuerier, Pool as ProtoPool, PoolAsset};

use num_rational::BigRational;
use num_traits::ToPrimitive;

use crate::CwDexError;

pub fn osmosis_calculate_join_pool_shares(
    querier: QuerierWrapper<OsmosisQuery>,
    pool_id: u64,
    assets: Vec<Coin>,
) -> StdResult<Coin> {
    let pool_state: PoolStateResponse =
        querier.query(&QueryRequest::Custom(OsmosisQuery::PoolState {
            id: pool_id,
        }))?;

    if assets.len() == 1 && pool_state.assets.iter().any(|c| c.denom == assets[0].denom) {
        let res = GammQuerier::new(&querier).pool(pool_id)?;
        let pool: ProtoPool = res
            .pool
            .ok_or_else(|| StdError::NotFound {
                kind: "pool".to_string(),
            })?
            .try_into() // convert `Any` to `osmosis_std::types::osmosis::gamm::v1beta1::Pool`
            .map_err(|e: DecodeError| StdError::ParseErr {
                target_type: "osmosis_std::types::osmosis::gamm::v1beta1::Pool".to_string(),
                msg: e.to_string(),
            })?;
        let swap_fee = Decimal::from_str(&pool.pool_params.as_ref().unwrap().swap_fee)?;
        let total_shares = Uint128::from_str(&pool.total_shares.as_ref().unwrap().amount)?;
        let denom = &pool.total_shares.as_ref().unwrap().denom;
        let pool_assets = &pool.pool_assets;
        let shares_out = calc_join_single_asset_tokens_in(
            pool.clone(),
            assets,
            total_shares,
            pool_assets.to_vec(),
            swap_fee,
        )?
        .0;
        Ok(Coin {
            denom: denom.to_string(),
            amount: shares_out,
        })
    } else if pool_state.assets.iter().all(|x| assets.iter().any(|y| x.denom == y.denom)) {
        let shares_out_amount = calc_join_pool_shares_double_sided(
            assets,
            pool_state.assets,
            pool_state.shares.amount,
        )?;

        Ok(Coin {
            denom: pool_state.shares.denom,
            amount: shares_out_amount,
        })
    } else {
        Err(StdError::generic_err("Provided assets must be either exactly one of the pool assets or all of the pool assets"))
    }
}

// func calcPoolSharesOutGivenSingleAssetIn(
// 	normalizedTokenWeightIn,
// ) sdk.Dec {
// 	// deduct swapfee on the in asset.
// 	// We don't charge swap fee on the token amount that we imagine as unswapped (the normalized weight).
// 	// So effective_swapfee = swapfee * (1 - normalized_token_weight)
// 	tokenAmountInAfterFee := tokenAmountIn.Mul(feeRatio(normalizedTokenWeightIn, swapFee))
// 	// To figure out the number of shares we add, first notice that in balancer we can treat
// 	// the number of shares as linearly related to the `k` value function. This is due to the normalization.
// 	// e.g.
// 	// if x^.5 y^.5 = k, then we `n` x the liquidity to `(nx)^.5 (ny)^.5 = nk = k'`
// 	// We generalize this linear relation to do the liquidity add for the not-all-asset case.
// 	// Suppose we increase the supply of x by x', so we want to solve for `k'/k`.
// 	// This is `(x + x')^{weight} * old_terms / (x^{weight} * old_terms) = (x + x')^{weight} / (x^{weight})`
// 	// The number of new shares we need to make is then `old_shares * ((k'/k) - 1)`
// 	// Whats very cool, is that this turns out to be the exact same `solveConstantFunctionInvariant` code
// 	// with the answer's sign reversed.
// 	poolAmountOut := solveConstantFunctionInvariant(
// 		tokenBalanceIn.Add(tokenAmountInAfterFee),
// 		tokenBalanceIn,
// 		normalizedTokenWeightIn,
// 		poolShares,
// 		sdk.OneDec()).Neg()
// 	return poolAmountOut
// }

fn calc_join_pool_shares_double_sided(
    provided_assets: Vec<Coin>,
    pool_assets: Vec<Coin>,
    total_shares: Uint128,
) -> StdResult<Uint128> {
    let provided_asset_1 = &provided_assets[0];
    let provided_asset_2 = &provided_assets[1];
    let provided_asset_1_pool_balance =
        pool_assets.iter().find(|c| c.denom == provided_asset_1.denom.to_string()).unwrap().amount;
    let provided_asset_2_pool_balance =
        pool_assets.iter().find(|c| c.denom == provided_asset_2.denom.to_string()).unwrap().amount;
    let shares_out_est_1 = provided_asset_1
        .amount
        .checked_mul(total_shares)?
        .checked_div(provided_asset_1_pool_balance)?;

    let shares_out_est_2 = provided_asset_2
        .amount
        .checked_mul(total_shares)?
        .checked_div(provided_asset_2_pool_balance)?;

    if shares_out_est_1 != shares_out_est_2 {
        Err(StdError::generic_err("assets being added to pool must be equal in value"))
    } else {
        Ok(shares_out_est_1)
    }
}

// feeRatio returns the fee ratio that is defined as follows:
// 1 - ((1 - normalizedTokenWeightOut) * swapFee)
fn _fee_ratio(normalized_weight: Decimal, swap_fee: Decimal) -> Decimal {
    Decimal::one().sub(Decimal::one().sub(normalized_weight) * swap_fee)
}

pub fn calc_pool_shares_out_given_single_asset_in(
    token_balance_in: Decimal,
    normalized_token_weight_in: Decimal,
    pool_shares: Decimal,
    token_amount_in: Decimal,
    swap_fee: Decimal,
) -> StdResult<BigRational> {
    // deduct swapfee on the in asset.
    // We don't charge swap fee on the token amount that we imagine as unswapped (the normalized weight).
    // So effective_swapfee = swapfee * (1 - normalized_token_weight)
    // tokenAmountInAfterFee := tokenAmountIn.Mul(feeRatio(normalizedTokenWeightIn, swap_fee))
    // To figure out the number of shares we add, first notice that in balancer we can treat
    // the number of shares as linearly related to the `k` value function. This is due to the normalization.
    // e.g.
    // if x^.5 y^.5 = k, then we `n` x the liquidity to `(nx)^.5 (ny)^.5 = nk = k'`
    // We generalize this linear relation to do the liquidity add for the not-all-asset case.
    // Suppose we increase the supply of x by x', so we want to solve for `k'/k`.
    // This is `(x + x')^{weight} * old_terms / (x^{weight} * old_terms) = (x + x')^{weight} / (x^{weight})`
    // The number of new shares we need to make is then `old_shares * ((k'/k) - 1)`
    // Whats very cool, is that this turns out to be the exact same `solveConstantFunctionInvariant` code
    // with the answer's sign reversed.
    // poolAmountOut := solveConstantFunctionInvariant(
    // 	tokenBalanceIn.Add(tokenAmountInAfterFee),
    // 	tokenBalanceIn,
    // 	normalizedTokenWeightIn,
    // 	poolShares,
    // 	sdk.OneDec()).Neg()

    let token_in_amount_after_fee =
        token_amount_in * _fee_ratio(normalized_token_weight_in, swap_fee);
    let pool_amount_out = osmosis_solve_constant_function_invariant(
        token_balance_in.checked_add(token_in_amount_after_fee)?,
        token_balance_in,
        normalized_token_weight_in,
        pool_shares,
        Decimal::one(),
    )?
    .neg();

    Ok(pool_amount_out)
}

// func (p *Pool) calcSingleAssetJoin(tokenIn sdk.Coin, swapFee sdk.Dec, tokenInPoolAsset PoolAsset, totalShares sdk.Int) (numShares sdk.Int, err error) {
// 	_, err = p.GetPoolAsset(tokenIn.Denom)
// 	if err != nil {
// 		return sdk.ZeroInt(), err
// 	}

// 	totalWeight := p.GetTotalWeight()
// 	if totalWeight.IsZero() {
// 		return sdk.ZeroInt(), errors.New("pool misconfigured, total weight = 0")
// 	}
// 	normalizedWeight := tokenInPoolAsset.Weight.ToDec().Quo(totalWeight.ToDec())
// 	return calcPoolSharesOutGivenSingleAssetIn(
// 		tokenInPoolAsset.Token.Amount.ToDec(),
// 		normalizedWeight,
// 		totalShares.ToDec(),
// 		tokenIn.Amount.ToDec(),
// 		swapFee,
// 	).TruncateInt(), nil
// }

#[cw_serde]
pub struct Pool {
    pub assets: Vec<PoolAsset>,
    pub total_weight: Uint128,
    pub total_shares: Coin,
    pub swap_fee: Decimal,
}

// impl TryFrom<ProtoPool> for Pool {
//     type Error = StdError;

//     fn try_from(proto_pool: ProtoPool) -> StdResult<Self> {
//         let assets = proto_pool
//             .pool_assets
//             .into_iter()
//             .map(|asset| PoolAsset::try_from(asset))
//             .collect::<StdResult<Vec<PoolAsset>>>()?;
//         let total_weight = Uint128::from_str(&proto_pool.total_weight)?;
//         let total_shares_proto =
//             proto_pool.total_shares.ok_or(StdError::generic_err("total shares not set"))?;
//         let total_shares = Coin {
//             amount: Uint128::from_str(total_shares_proto.amount.as_str())?,
//             denom: total_shares_proto.denom,
//         };

//         let pool_params =
//             proto_pool.pool_params.ok_or(StdError::generic_err("pool params not set"))?;

//         Ok(Pool {
//             assets,
//             total_weight,
//             total_shares,
//             swap_fee: Decimal::from_str(&pool_params.swap_fee)?,
//         })
//     }
// }

// #[cw_serde]
// pub struct PoolAsset {
//     pub token: Coin,
//     pub weight: Uint128,
// }

// impl TryFrom<ProtoPoolAsset> for PoolAsset {
//     type Error = StdError;

//     fn try_from(proto_pool_asset: ProtoPoolAsset) -> StdResult<Self> {
//         let proto_coin = proto_pool_asset.token.ok_or(StdError::generic_err("token is missing"))?;
//         Ok(PoolAsset {
//             token: Coin {
//                 amount: Uint128::from_str(proto_coin.amount.as_str())?,
//                 denom: proto_coin.denom,
//             },
//             weight: Uint128::from_str(proto_pool_asset.weight.as_str())?,
//         })
//     }
// }

pub fn calc_single_asset_join(
    pool: ProtoPool,
    token_in: &Coin,
    swap_fee: Decimal,
    token_in_pool_asset: &PoolAsset,
    total_shares: Uint128,
) -> StdResult<Uint128> {
    // 	_, err = p.GetPoolAsset(tokenIn.Denom)
    // 	if err != nil {
    // 		return sdk.ZeroInt(), err
    // 	}

    let total_weight = Uint128::from_str(&pool.total_weight)?;
    if total_weight.is_zero() {
        return Err(StdError::generic_err("pool misconfigured, total weight = 0"));
    }

    let normalized_weight =
        Decimal::from_ratio(Uint128::from_str(&token_in_pool_asset.weight)?, total_weight);
    calc_pool_shares_out_given_single_asset_in(
        Decimal::from_ratio(
            Uint128::from_str(&token_in_pool_asset.token.as_ref().unwrap().amount)?,
            Uint128::from(1u128),
        ),
        normalized_weight,
        Decimal::from_ratio(total_shares, Uint128::from(1u128)),
        Decimal::from_ratio(token_in.amount, Uint128::from(1u128)),
        swap_fee,
    )?
    .to_integer()
    .to_u128()
    .map(Uint128::from)
    .ok_or(StdError::from(CwDexError::BigIntOverflow {}))
}

// calcJoinSingleAssetTokensIn attempts to calculate single
// asset join for all tokensIn given totalShares in pool,
// poolAssetsByDenom and swapFee. totalShares is the number
// of shares in pool before beginnning to join any of the tokensIn.
//
// Returns totalNewShares and totalNewLiquidity from joining all tokensIn
// by mimicking individually single asset joining each.
// or error if fails to calculate join for any of the tokensIn.
// func (p *Pool) calcJoinSingleAssetTokensIn(tokensIn sdk.Coins, totalShares sdk.Int, poolAssetsByDenom map[string]PoolAsset, swapFee sdk.Dec) (sdk.Int, sdk.Coins, error) {
// 	totalNewShares := sdk.ZeroInt()
// 	totalNewLiquidity := sdk.NewCoins()
// 	for _, coin := range tokensIn {
// 		newShares, err := p.calcSingleAssetJoin(coin, swapFee, poolAssetsByDenom[coin.Denom], totalShares.Add(totalNewShares))
// 		if err != nil {
// 			return sdk.ZeroInt(), sdk.Coins{}, err
// 		}

// 		totalNewLiquidity = totalNewLiquidity.Add(coin)
// 		totalNewShares = totalNewShares.Add(newShares)
// 	}
// 	return totalNewShares, totalNewLiquidity, nil
// }

pub fn calc_join_single_asset_tokens_in(
    pool: ProtoPool,
    tokens_in: Vec<Coin>,
    total_shares: Uint128,
    pool_assets: Vec<PoolAsset>,
    swap_fee: Decimal,
) -> StdResult<(Uint128, Vec<Coin>)> {
    let mut total_new_shares = Uint128::zero();
    let mut total_new_liquidity = vec![];
    for coin in tokens_in {
        let new_shares = calc_single_asset_join(
            pool.clone(),
            &coin,
            swap_fee,
            pool_assets
                .iter()
                .find(|pool_asset| pool_asset.token.as_ref().unwrap().denom == coin.denom)
                .ok_or(StdError::generic_err("pool asset not found"))?,
            total_shares.checked_add(total_new_shares)?,
        )?;

        total_new_liquidity.push(coin);
        total_new_shares = total_new_shares.checked_add(new_shares)?;
    }
    Ok((total_new_shares, total_new_liquidity))
}

/// Calculates the [[`Coin`]] amounts that will be returned when withdrawing `exit_share_amount` LP shares from the pool
/// with pool id `pool_id` on Osmosis. The implementation is a translation of the calculations performed in the Go code
/// of the GAMM module. See
/// https://github.com/osmosis-labs/osmosis/blob/91c7830d7d195aad53378d60b24224a67e70fd7f/x/gamm/pool-models/internal/cfmm_common/lp.go#L16
pub fn osmosis_calculate_exit_pool_amounts(
    querier: QuerierWrapper<OsmosisQuery>,
    pool_id: u64,
    exit_lp_shares: &Coin,
) -> StdResult<Vec<Coin>> {
    // TODO: Remove go code comments after review
    let res = GammQuerier::new(&querier).pool(pool_id)?;
    let pool: ProtoPool = res
        .pool
        .ok_or_else(|| StdError::NotFound {
            kind: "pool".to_string(),
        })?
        .try_into() // convert `Any` to `osmosis_std::types::osmosis::gamm::v1beta1::Pool`
        .map_err(|e: DecodeError| StdError::ParseErr {
            target_type: "osmosis_std::types::osmosis::gamm::v1beta1::Pool".to_string(),
            msg: e.to_string(),
        })?;
    let pool_state: PoolStateResponse =
        querier.query(&QueryRequest::Custom(OsmosisQuery::PoolState {
            id: pool_id,
        }))?;

    if exit_lp_shares.denom != pool_state.shares.denom {
        return Err(StdError::generic_err(format!(
            "exit_shares denom {} does not match pool lp shares denom {}",
            exit_lp_shares.denom, pool_state.shares.denom
        )));
    }

    let exit_fee = Decimal::from_str(&pool.pool_params.unwrap().exit_fee)?;

    // totalShares := pool.GetTotalShares()
    // if exitingShares.GTE(totalShares) {
    // 	return sdk.Coins{}, sdkerrors.Wrapf(types.ErrLimitMaxAmount, errMsgFormatSharesLargerThanMax, exitingShares, totalShares)
    // }

    let total_shares = pool_state.shares.amount;
    if exit_lp_shares.amount >= total_shares {
        return Err(StdError::generic_err("exit share amount must be less than total shares"));
    }

    // // refundedShares = exitingShares * (1 - exit fee)
    // // with 0 exit fee optimization
    // var refundedShares sdk.Dec
    // if !exitFee.IsZero() {
    // 	// exitingShares * (1 - exit fee)
    // 	oneSubExitFee := sdk.OneDec().SubMut(exitFee)
    // 	refundedShares = oneSubExitFee.MulIntMut(exitingShares)
    // } else {
    // 	refundedShares = exitingShares.ToDec()
    // }

    let refunded_shares: Decimal;
    if !exit_fee.is_zero() {
        refunded_shares = Decimal::from_ratio(exit_lp_shares.amount, 1u128)
            .checked_mul(Decimal::one() - exit_fee)?;
    } else {
        refunded_shares = Decimal::from_ratio(exit_lp_shares.amount, 1u128);
    }

    // shareOutRatio := refundedShares.QuoInt(totalShares)

    let share_out_ratio = refunded_shares.checked_mul(Decimal::from_ratio(1u128, total_shares))?;

    // // exitedCoins = shareOutRatio * pool liquidity
    // exitedCoins := sdk.Coins{}
    // poolLiquidity := pool.GetTotalPoolLiquidity(ctx)
    // for _, asset := range poolLiquidity {
    // 	// round down here, due to not wanting to over-exit
    // 	exitAmt := shareOutRatio.MulInt(asset.Amount).TruncateInt()
    // 	if exitAmt.LTE(sdk.ZeroInt()) {
    // 		continue
    // 	}
    // 	if exitAmt.GTE(asset.Amount) {
    // 		return sdk.Coins{}, errors.New("too many shares out")
    // 	}
    // 	exitedCoins = exitedCoins.Add(sdk.NewCoin(asset.Denom, exitAmt))
    // }

    let mut exited_coins: Vec<Coin> = vec![];
    for pool_asset in pool_state.assets {
        let exit_amount = share_out_ratio * pool_asset.amount;
        if exit_amount.is_zero() {
            continue;
        }
        if exit_amount >= pool_asset.amount {
            return Err(StdError::generic_err("too many shares out"));
        }
        exited_coins.push(Coin {
            denom: pool_asset.denom,
            amount: exit_amount,
        });
    }

    // return exitedCoins, nil

    Ok(exited_coins)
}

// // weightRatio = (weightX/weightY)
// weightRatio := tokenWeightFixed.Quo(tokenWeightUnknown)

// // y = balanceXBefore/balanceXAfter
// y := tokenBalanceFixedBefore.Quo(tokenBalanceFixedAfter)

// // amountY = balanceY * (1 - (y ^ weightRatio))
// yToWeightRatio := osmomath.Pow(y, weightRatio)
// paranthetical := sdk.OneDec().Sub(yToWeightRatio)
// amountY := tokenBalanceUnknownBefore.Mul(paranthetical)
// return amountY

/// Translation of the solveConstantFunctionInvariant function in the osmosis go code.
/// The y_to_weight_ratio calculation is a workaround that works only for dual pools with
/// even weight of the two assets. Go function in osmosis code can be found here:
/// https://github.com/osmosis-labs/osmosis/blob/main/x/gamm/pool-models/balancer/amm.go#L94
fn osmosis_solve_constant_function_invariant(
    token_balance_fixed_before: Decimal,
    token_balance_fixed_after: Decimal,
    token_weight_fixed: Decimal,
    token_balance_unknown_before: Decimal,
    token_weight_unknown: Decimal,
) -> StdResult<BigRational> {
    // // weightRatio = (weightX/weightY)
    // weightRatio := tokenWeightFixed.Quo(tokenWeightUnknown)
    let weight_ratio = token_weight_fixed / token_weight_unknown;

    // // y = balanceXBefore/balanceXAfter
    // y := tokenBalanceFixedBefore.Quo(tokenBalanceFixedAfter)
    // let y = Decimal::from_ratio(token_balance_fixed_before, token_balance_fixed_after);
    // let y = BigRational::new_raw(
    //     token_balance_fixed_before.u128().into(),
    //     token_balance_fixed_after.u128().into(),
    // );
    let y = token_balance_fixed_before / token_balance_fixed_after;

    // // amountY = balanceY * (1 - (y ^ weightRatio))
    // yToWeightRatio := osmomath.Pow(y, weightRatio)
    // paranthetical := sdk.OneDec().Sub(yToWeightRatio)
    // amountY := tokenBalanceUnknownBefore.Mul(paranthetical)
    // return amountY
    let y_to_weight_ratio = _osmosis_pow(y, weight_ratio)?;
    let paranthetical = BigRational::new_raw(1u128.into(), 1u128.into()) - y_to_weight_ratio;
    let amount_y = decimal_to_bigrational(token_balance_unknown_before) * paranthetical;
    return Ok(amount_y);
}

fn decimal_to_bigrational(decimal: Decimal) -> BigRational {
    let denom: BigInt = 10u128.pow(Decimal::DECIMAL_PLACES).into();
    BigRational::new_raw(decimal.atomics().u128().into(), denom.clone())
}

fn _osmosis_pow(base: Decimal, exp: Decimal) -> StdResult<BigRational> {
    let base_big = decimal_to_bigrational(base);
    if base_big >= BigRational::new_raw(2u128.into(), 1u128.into()) {
        return Err(StdError::generic_err("base must be lesser than two"));
    }

    // // We will use an approximation algorithm to compute the power.
    // // Since computing an integer power is easy, we split up the exponent into
    // // an integer component and a fractional component.
    // integer := exp.TruncateDec()
    // fractional := exp.Sub(integer)

    // let integer = exp.to_integer();
    // let fractional = exp - BigRational::new_raw(integer.clone(), 1u128.into());
    let integer = exp * Uint128::one();
    let fractional = exp - Decimal::from_ratio(integer, 1u128);

    // integerPow := base.Power(uint64(integer.TruncateInt64()))
    let integer_pow = base_big.pow(
        integer
            .u128()
            .try_into()
            .map_err(|x| StdError::generic_err(format!("integer conversion failed: {}", x)))?,
    );

    // if fractional.IsZero() {
    // 	return integerPow
    // }
    if fractional.is_zero() {
        return Ok(integer_pow);
    }

    let pow_precision = Decimal::from_ratio(1u128, 100000000u128);

    // fractionalPow := PowApprox(base, fractional, powPrecision)
    let fractional_pow = _osmosis_pow_approx(base, fractional, pow_precision.clone())?;

    // return integerPow.Mul(fractionalPow)
    return Ok(integer_pow * fractional_pow);
}

// Contract: 0 < base <= 2
// 0 <= exp < 1.
fn _osmosis_pow_approx(base: Decimal, exp: Decimal, precision: Decimal) -> StdResult<BigRational> {
    let one: BigRational = BigRational::from_integer(1u128.into());
    if exp.is_zero() {
        return Ok(one);
    }

    // Common case optimization
    // Optimize for it being equal to one-half
    // if exp == BigRational::new_raw(1u128.into(), 2u128.into()) {
    // let numer = base.numer().sqrt();
    // let denom = base.denom().sqrt();
    // return BigRational::new_raw(numer, denom);
    // }
    // TODO: Make an approx-equal function, and then check if exp * 3 = 1, and do a check accordingly

    // We compute this via taking the maclaurin series of (1 + x)^a
    // where x = base - 1.
    // The maclaurin series of (1 + x)^a = sum_{k=0}^{infty} binom(a, k) x^k
    // Binom(a, k) takes the natural continuation on the first parameter, namely that
    // Binom(a, k) = N/D, where D = k!, and N = a(a-1)(a-2)...(a-k+1)
    // Next we show that the absolute value of each term is less than the last term.
    // Note that the change in term n's value vs term n + 1 is a multiplicative factor of
    // v_n = x(a - n) / (n+1)
    // So if |v_n| < 1, we know that each term has a lesser impact on the result than the last.
    // For our bounds on |x| < 1, |a| < 1,
    // it suffices to see for what n is |v_n| < 1,
    // in the worst parameterization of x = 1, a = -1.
    // v_n = |(-1 + epsilon - n) / (n+1)|
    // So |v_n| is always less than 1, as n ranges over the integers.
    //
    // Note that term_n of the expansion is 1 * prod_{i=0}^{n-1} v_i
    // The error if we stop the expansion at term_n is:
    // error_n = sum_{k=n+1}^{infty} term_k
    // At this point we further restrict a >= 0, so 0 <= a < 1.
    // Now we take the _INCORRECT_ assumption that if term_n < p, then
    // error_n < p.
    // This assumption is obviously wrong.
    // However our usages of this function don't use the full domain.
    // With a > 0, |x| << 1, and p sufficiently low, perhaps this actually is true.

    // TODO: Check with our parameterization
    // TODO: If theres a bug, balancer is also wrong here :thonk:

    // base = base.Clone()
    // x, xneg := AbsDifferenceWithSign(base, one)
    // term := sdk.OneDec()
    // sum := sdk.OneDec()
    // negative := false
    let (x, x_neg) = _osmosis_abs_difference_with_sign(base, Decimal::one());
    let mut term = Decimal::one();
    let mut sum = one;
    let mut negative = false;

    // a := exp.Clone()
    // bigK := sdk.NewDec(0)
    let mut a = exp.clone();
    let mut big_k = Decimal::zero();

    println!("precision: {}", precision.to_string());
    // for i := int64(1); term.GTE(precision); i++ {
    let mut i: i64 = 1;
    loop {
        // println!("term: {}", term.to_string());

        if term < precision {
            break;
        }

        // // At each iteration, we need two values, i and i-1.
        // // To avoid expensive big.Int allocation, we reuse bigK variable.
        // // On this line, bigK == i-1.
        // c, cneg := AbsDifferenceWithSign(a, bigK)
        let (c, c_neg) = _osmosis_abs_difference_with_sign(a, big_k.clone());

        // // On this line, bigK == i.
        // bigK.Set(sdk.NewDec(i))
        // term.MulMut(c).MulMut(x).QuoMut(bigK)
        // big_k = BigRational::new_raw(i.into(), 1u128.into());
        big_k = Decimal::from_ratio(i as u128, 1u128);
        term = mul_mut(term, c)?;
        term = mul_mut(term, x)?;
        term /= big_k.clone();
        // term = quo_mut(term, big_k)?

        // // a is mutated on absDifferenceWithSign, reset
        // a.Set(exp)

        // a is never mutated in our implementation. i think we can remove it and use exp directly.
        a = exp.clone();

        // if term.isZero() {
        //     break;
        // }
        if term.is_zero() {
            break;
        }

        // if xneg {
        //     negative = !negative
        // }
        if x_neg {
            negative = !negative;
        }

        // if cneg {
        //     negative = !negative
        // }
        if c_neg {
            negative = !negative;
        }

        // if negative {
        //     sum.SubMut(term)
        // } else {
        //     sum.AddMut(term)
        // }
        if negative {
            sum -= decimal_to_bigrational(term);
        } else {
            sum += decimal_to_bigrational(term);
        }

        i += 1;
    }
    return Ok(sum);
}

//Don't ask...
fn chop_precision_and_round(d: u128) -> u128 {
    // 	// get the truncated quotient and remainder
    // quo, rem := d, big.NewInt(0)
    // quo, rem = quo.QuoRem(d, precisionReuse, rem)

    // if rem.Sign() == 0 { // remainder is zero
    // 	return quo
    // }

    // switch rem.Cmp(fivePrecision) {
    // case -1:
    // 	return quo
    // case 1:
    // 	return quo.Add(quo, oneInt)
    // default: // bankers rounding must take place
    // 	// always round to an even number
    // 	if quo.Bit(0) == 0 {
    // 		return quo
    // 	}
    // 	return quo.Add(quo, oneInt)
    // }

    let precision_reuse = 10u128.pow(18);
    let five_precision = precision_reuse / 2;

    let quo = d / precision_reuse;
    let rem = d - precision_reuse * quo;

    if rem == 0 {
        return quo;
    }

    if rem < five_precision {
        return quo;
    } else if rem > five_precision {
        return quo + 1;
    } else {
        if ((quo >> 0) & 1) == 0 {
            return quo;
        } else {
            return quo + 1;
        }
    }
}

fn mul_mut(d: Decimal, d2: Decimal) -> StdResult<Decimal> {
    let mut di = d.atomics();
    di *= d2.atomics();

    let chopped = chop_precision_and_round(di.u128());

    Decimal::from_atomics(chopped, Decimal::DECIMAL_PLACES).map_err(|e| {
        StdError::generic_err(format!("Error converting from atomics to decimal: {}", e))
    })
}

fn _quo_mut(d: Decimal, d2: Decimal) -> StdResult<Decimal> {
    // multiply precision twice
    // d.i.Mul(d.i, precisionReuse)
    // d.i.Mul(d.i, precisionReuse)
    // d.i.Quo(d.i, d2.i)

    // chopPrecisionAndRound(d.i)
    // if d.i.BitLen() > maxDecBitLen {
    // 	panic("Int overflow")
    // }
    // return d

    let mut di = d.atomics().u128();
    di *= 10u128.pow(18);
    di *= 10u128.pow(18);
    di /= d2.atomics().u128();

    let chopped = chop_precision_and_round(di);

    Decimal::from_atomics(chopped, Decimal::DECIMAL_PLACES).map_err(|e| {
        StdError::generic_err(format!("Error converting from atomics to decimal: {}", e))
    })
}

// AbsDifferenceWithSign returns | a - b |, (a - b).sign()
// a is mutated and returned.
fn _osmosis_abs_difference_with_sign(a: Decimal, b: Decimal) -> (Decimal, bool) {
    if a >= b {
        (a - b, false)
    } else {
        (b - a, true)
    }
}

// TODO: Tests for JoinPool and ExitPool

#[cfg(test)]
mod tests {
    // use super::*;

    // // // #[test_case(1, vec!["uosmo".to_string(), "uatom".to_string()], Decimal::from_ratio(1u8,50u8), Decimal::from_ratio(1u8,500u8), 1, 0.5;"test_join_pool_calculation_single_sided")]
    // // // fn test_join_pool_calculation_single_sided(
    // // //     num_accounts: u64,
    // // //     pool_names: Vec<String>,
    // // //     base: Decimal,
    // // //     precision: Decimal,
    // // //     exp: Decimal,
    // // //     expected: Decimal,
    // // // ) {
    // // //     let actual = join_pool_calculation(num_accounts, pool_names, base, precision, exp, false);
    // // //     assert_eq!(actual, expected);
    // // // }

    // #[derive(Clone)]
    // struct CalcJoinSharesTestCase {
    //     pub name: String,
    //     pub swap_fee: Decimal,
    //     pub pool_assets: Vec<PoolAsset>,
    //     pub tokens_in: Vec<Coin>,
    //     pub expect_shares: Uint128,
    // }

    // #[test]
    // fn test_osmosis_calculate_join_pool_shares_single_sided() {
    //     let one_trillion: u128 = 1e12 as u128;
    //     let default_osmo_pool_asset: PoolAsset = PoolAsset {
    //         token: Coin::new(one_trillion, "uosmo").into(),
    //         weight: Uint128::new(100),
    //     };
    //     let default_atom_pool_asset: PoolAsset = PoolAsset {
    //         token: Coin::new(one_trillion, "uatom").into(),
    //         weight: Uint128::new(100),
    //     };
    //     let one_trillion_even_pool_assets: Vec<PoolAsset> =
    //         vec![default_osmo_pool_asset.clone(), default_atom_pool_asset.clone()];

    //     let calc_single_asset_join_test_cases: Vec<CalcJoinSharesTestCase> = vec![
    //     CalcJoinSharesTestCase {
    //         name:         "single tokens_in - equal weights with zero swap fee".to_string(),
    //         swap_fee:      Decimal::zero(),
    //         pool_assets:   one_trillion_even_pool_assets.clone(),
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(2_499_999_968_750),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:         "single tokens_in - equal weights with 0.01 swap fee".to_string(),
    //         swap_fee:      Decimal::from_str("0.01").unwrap(),
    //         pool_assets:   one_trillion_even_pool_assets.clone(),
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(2_487_500_000_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:         "single tokens_in - equal weights with 0.99 swap fee".to_string(),
    //         swap_fee:      Decimal::from_str("0.99").unwrap(),
    //         pool_assets:   one_trillion_even_pool_assets.clone(),
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(1_262_500_000_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single tokens_in - unequal weights with 0.99 swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0.99").unwrap(),
    //         pool_assets: vec![
    //             default_osmo_pool_asset.clone(),
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(300),
    //             },
    //         ],
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(321_875_000_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - token in weight is greater than the other token, with zero swap fee".to_string(),
    //         swap_fee: Decimal::zero(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uosmo"),
    //                 weight: Uint128::new(500),
    //             },
    //             default_atom_pool_asset.clone(),
    //         ],
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(4_166_666_649_306),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - token in weight is greater than the other token, with non-zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0.01").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uosmo"),
    //                 weight: Uint128::new(500),
    //             },
    //             default_atom_pool_asset.clone(),
    //         ],
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(4_159_722_200_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - token in weight is smaller than the other token, with zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uosmo"),
    //                 weight: Uint128::new(200),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(1000),
    //             },
    //         ],
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(833_333_315_972),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - token in weight is smaller than the other token, with non-zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0.02").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uosmo"),
    //                 weight: Uint128::new(200),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(1000),
    //             },
    //         ],
    //         tokens_in:     vec![Coin::new(50_000, "uosmo")],
    //         expect_shares: Uint128::new(819_444_430_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - tokenIn is large relative to liquidity, token in weight is smaller than the other token, with zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(156_736, "uosmo"),
    //                 weight: Uint128::new(200),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(1000),
    //             },
    //         ],
    //         // 156_736 * 3 / 4 = 117552
    //         tokens_in: vec![Coin::new(117552, "uosmo")],
    //         expect_shares: Uint128::new(9_775_731_930_496_140_648),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - tokenIn is large relative to liquidity, token in weight is smaller than the other token, with non-zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0.02").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(156_736, "uosmo"),
    //                 weight: Uint128::new(200),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(1000),
    //             },
    //         ],
    //         // 156_736 / 4 * 3 = 117552
    //         tokens_in: vec![Coin::new(117552, "uosmo")],
    //         expect_shares: Uint128::new(9_644_655_900_000_000_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         name:    "single asset - (almost 1 == tokenIn / liquidity ratio), token in weight is smaller than the other token, with zero swap fee".to_string(),
    //         swap_fee: Decimal::from_str("0").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(500_000, "uosmo"),
    //                 weight: Uint128::new(100),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(1000),
    //             },
    //         ],
    //         tokens_in: vec![Coin::new(499_999, "uosmo")],
    //         expect_shares: Uint128::new(6_504_099_261_800_144_638),
    //     },
    //     // TODO: Handle error and panic cases
    //     // CalcJoinSharesTestCase {
    //     //     // Currently, our Pow approximation function does not work correctly when one tries
    //     //     // to add liquidity that is larger than the existing liquidity.
    //     //     // The ratio of tokenIn / existing liquidity that is larger than or equal to 1 causes a panic.
    //     //     // This has been deemed as acceptable since it causes code complexity to fix
    //     //     // & only affects UX in an edge case (user has to split up single asset joins)
    //     //     name:    "single asset - (exactly 1 == tokenIn / liquidity ratio - failure), token in weight is smaller than the other token, with zero swap fee".to_string(),
    //     //     swap_fee: Decimal::from_str("0").unwrap(),
    //     //     pool_assets: vec![
    //     //         PoolAsset {
    //     //             token:  Coin::new(500_000, "uosmo"),
    //     //             weight: Uint128::new(100),
    //     //         },
    //     //         PoolAsset {
    //     //             token:  Coin::new(one_trillion, "uatom"),
    //     //             weight: Uint128::new(1000),
    //     //         },
    //     //     ],
    //     //     tokens_in: vec![Coin::new(500_000, "uosmo")],
    //     //     expect_shares: Uint128::new(6_504_099_261_800_144_638),
    //     //     expectPanic:  true,
    //     // },
    //     // CalcJoinSharesTestCase {
    //     //     name:         "tokenIn asset does not exist in pool",
    //     //     swap_fee:      Decimal::from_str("0"),
    //     //     pool_assets:   one_trillion_even_pool_assets,
    //     //     tokens_in:     vec![](Uint128::new64Coin(doesNotExistDenom, 50_000)),
    //     //     expect_shares: sdk.ZeroInt(),
    //     //     expErr:       sdkerrors.Wrapf(types.ErrDenomNotFoundInPool, fmt.Sprintf(balancer.ErrMsgFormatNoPoolAssetFound, doesNotExistDenom)),
    //     // },
    //     CalcJoinSharesTestCase {
    //         // Pool liquidity is changed by 1e-12 / 2
    //         // P_issued = 1e20 * 1e-12 / 2 = 1e8 / 2 = 50_000_000
    //         name:    "minimum input single asset equal liquidity".to_string(),
    //         swap_fee: Decimal::from_str("0").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uosmo"),
    //                 weight: Uint128::new(100),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(one_trillion, "uatom"),
    //                 weight: Uint128::new(100),
    //             },
    //         ],
    //         tokens_in: vec![Coin::new(1, "uosmo")],
    //         expect_shares: Uint128::new(50_000_000),
    //     },
    //     CalcJoinSharesTestCase {
    //         // P_issued should be 1/10th that of the previous test
    //         // p_issued = 50_000_000 / 10 = 5_000_000
    //         name:    "minimum input single asset imbalanced liquidity".to_string(),
    //         swap_fee: Decimal::from_str("0").unwrap(),
    //         pool_assets: vec![
    //             PoolAsset {
    //                 token:  Coin::new(10_000_000_000_000, "uosmo"),
    //                 weight: Uint128::new(100),
    //             },
    //             PoolAsset {
    //                 token:  Coin::new(1_000_000_000_000, "uatom"),
    //                 weight: Uint128::new(100),
    //             },
    //         ],
    //         tokens_in: vec![Coin::new(1, "uosmo")],
    //         expect_shares: Uint128::new(5_000_000),
    //     }];

    //     // func assertExpectedSharesErrRatio(t *testing.T, expectedShares, actualShares sdk.Int) {
    //     //     allowedErrRatioDec, err := sdk.NewDecFromStr(allowedErrRatio)
    //     //     require.NoError(t, err)

    //     //     errTolerance := osmoutils.ErrTolerance{
    //     //         MultiplicativeTolerance: allowedErrRatioDec,
    //     //     }

    //     //     require.Equal(
    //     //         t,
    //     //         0,
    //     //         errTolerance.Compare(expectedShares, actualShares),
    //     //         fmt.Sprintf("expectedShares: %s, actualShares: %s", expectedShares.String(), actualShares.String()))
    //     // }

    //     fn assert_expected_shares_err_ratio(expected_shares: Uint128, actual_shares: Uint128) {
    //         fn compare(expected: Uint128, actual: Uint128) -> i8 {
    //             let allowed_err_ratio_dec = Decimal::from_str("0.0000001").unwrap();
    //             let multiplicative_tolerance = allowed_err_ratio_dec;
    //             let diff = if expected > actual {
    //                 expected - actual
    //             } else {
    //                 actual - expected
    //             };

    //             let comparison_sign = if expected > actual {
    //                 1
    //             } else {
    //                 -1
    //             };

    //             //Check multiplicative tolerance equations
    //             if !multiplicative_tolerance.is_zero() {
    //                 // let err_term = diff.to_decimal() / Decimal::from(expected.min(actual));
    //                 let err_term = Decimal::from_ratio(diff, expected.min(actual));
    //                 if err_term > multiplicative_tolerance {
    //                     return comparison_sign;
    //                 }
    //             }
    //             return 0;
    //         }

    //         assert_eq!(0, compare(expected_shares, actual_shares));
    //     }

    //     for (id, test_case) in calc_single_asset_join_test_cases.into_iter().enumerate() {
    //         let expected_new_liquidity = test_case.tokens_in.clone();

    //         let one_share = Uint128::from(10u128.pow(18));
    //         let init_pool_shares_supply = one_share * Uint128::from(100u128);

    //         let pool = Pool {
    //             assets: test_case.pool_assets.clone(),
    //             total_weight: test_case.pool_assets.iter().map(|a| a.weight).sum(),
    //             total_shares: Coin {
    //                 denom: test_case
    //                     .pool_assets
    //                     .iter()
    //                     .fold("".to_string(), |acc, a| acc + &a.token.denom + " "),
    //                 amount: init_pool_shares_supply,
    //             },
    //             swap_fee: test_case.swap_fee,
    //         };

    //         let (total_num_shares, total_new_liquidity) = calc_join_single_asset_tokens_in(
    //             pool.clone(),
    //             test_case.tokens_in,
    //             pool.total_shares.amount,
    //             test_case.pool_assets,
    //             test_case.swap_fee,
    //         )
    //         .unwrap();

    //         println!("Running test for Test case id {}, name: {}", id, test_case.name);

    //         assert_eq!(expected_new_liquidity, total_new_liquidity);

    //         assert_expected_shares_err_ratio(test_case.expect_shares, total_num_shares);
    //     }
    // }
}
