use std::{
    ops::{Neg, Sub},
    str::FromStr,
};

use apollo_proto_rust::osmosis::gamm::v1beta1::{Pool, PoolAsset};
use cosmwasm_std::{Coin, Decimal, QuerierWrapper, QueryRequest, StdError, StdResult, Uint128};
use cw20_base::ContractError;
use num_bigint::BigInt;
use osmo_bindings::{OsmosisQuery, PoolStateResponse};

use num_rational::BigRational;
use num_traits::{identities::One, ToPrimitive};

use crate::CwDexError;

use super::helpers::query_pool_params;

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
        todo!("Calculate single asset join pool shares")
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
    token_balance_in: Uint128,
    normalized_token_weight_in: Decimal,
	poolShares,
	tokenAmountIn,
	swapFee sdk.Dec,
    swap_fee: Decimal,
) -> StdResult<Uint128> {
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
        token_balance_in * _fee_ratio(provided_asset_normalized_weight, swap_fee);
    let pool_amount_out = osmosis_solve_constant_function_invariant(
        provided_asset_pool_balance.checked_add(token_in_amount_after_fee)?,
        provided_asset_pool_balance,
        provided_asset_normalized_weight,
        total_shares,
        Decimal::one(),
    )?
    .neg();
    // TODO: Is this going to run into a negative number and cause a crash?

    pool_amount_out
        .to_u128()
        .map(Uint128::from)
        .ok_or(StdError::from(CwDexError::BigIntOverflow {}))
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

pub fn calc_single_asset_join(
    pool: Pool,
    token_in: &Coin,
    swap_fee: Decimal,
    token_in_pool_asset: &PoolAsset,
    total_shares: Uint128,
) -> StdResult<Uint128> {
    // 	_, err = p.GetPoolAsset(tokenIn.Denom)
    // 	if err != nil {
    // 		return sdk.ZeroInt(), err
    // 	}

    let total_weight = Uint128::from_str(pool.total_weight.as_str())?;
    if total_weight.is_zero() {
        return Err(StdError::generic_err("pool misconfigured, total weight = 0"));
    }

    let token_in_pool_asset_weight = Uint128::from_str(token_in_pool_asset.weight.as_str())?;
    let normalized_weight = Decimal::from_ratio(token_in_pool_asset_weight, total_weight);
    calc_pool_shares_out_given_single_asset_in(
        token_in,
        total_shares,
        token_in_pool_asset.token.amount,
        normalized_weight,
        swap_fee,
    )
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

    let pool_params = query_pool_params(querier, pool_id)?;
    let exit_fee = Decimal::from_str(&pool_params.exit_fee)?;

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
    token_balance_fixed_before: Uint128,
    token_balance_fixed_after: Uint128,
    token_weight_fixed: Decimal,
    token_balance_unknown_before: Uint128,
    token_weight_unknown: Decimal,
) -> StdResult<BigInt> {
    // // weightRatio = (weightX/weightY)
    // weightRatio := tokenWeightFixed.Quo(tokenWeightUnknown)
    let weight_ratio =
        decimal_to_bigrational(token_weight_fixed) / decimal_to_bigrational(token_weight_unknown);

    // // y = balanceXBefore/balanceXAfter
    // y := tokenBalanceFixedBefore.Quo(tokenBalanceFixedAfter)
    // let y = Decimal::from_ratio(token_balance_fixed_before, token_balance_fixed_after);
    let y = BigRational::new_raw(
        token_balance_fixed_before.u128().into(),
        token_balance_fixed_after.u128().into(),
    );

    // // amountY = balanceY * (1 - (y ^ weightRatio))
    // yToWeightRatio := osmomath.Pow(y, weightRatio)
    // paranthetical := sdk.OneDec().Sub(yToWeightRatio)
    // amountY := tokenBalanceUnknownBefore.Mul(paranthetical)
    // return amountY
    let y_to_weight_ratio = _osmosis_pow(y, weight_ratio)?;
    let paranthetical = BigRational::new_raw(1u128.into(), 1u128.into()) - y_to_weight_ratio;
    let amount_y = BigRational::new_raw(token_balance_unknown_before.u128().into(), 1u128.into())
        * paranthetical;
    return Ok(amount_y.to_integer());
}

fn decimal_to_bigrational(decimal: Decimal) -> BigRational {
    let denom: BigInt = 10u128.pow(Decimal::DECIMAL_PLACES).into();
    BigRational::new_raw(decimal.atomics().u128().into(), denom.clone())
}

fn _osmosis_pow(base: BigRational, exp: BigRational) -> StdResult<BigRational> {
    if base >= BigRational::new_raw(2u128.into(), 1u128.into()) {
        return Err(StdError::generic_err("base must be lesser than two"));
    }

    // // We will use an approximation algorithm to compute the power.
    // // Since computing an integer power is easy, we split up the exponent into
    // // an integer component and a fractional component.
    // integer := exp.TruncateDec()
    // fractional := exp.Sub(integer)
    let integer = exp.to_integer();
    let fractional = exp - BigRational::new_raw(integer.clone(), 1u128.into());

    // integerPow := base.Power(uint64(integer.TruncateInt64()))
    let integer_pow = base.pow(
        integer
            .try_into()
            .map_err(|x| StdError::generic_err(format!("integer conversion failed: {}", x)))?,
    );

    // if fractional.IsZero() {
    // 	return integerPow
    // }
    if fractional == BigRational::new_raw(0u128.into(), 1u128.into()) {
        return Ok(integer_pow);
    }

    let pow_precision: BigRational = BigRational::new_raw(1u128.into(), 100000000u128.into());

    // fractionalPow := PowApprox(base, fractional, powPrecision)
    let fractional_pow = _osmosis_pow_approx(base, fractional, pow_precision.clone());

    // return integerPow.Mul(fractionalPow)
    return Ok(integer_pow * fractional_pow);
}

// Contract: 0 < base <= 2
// 0 <= exp < 1.
fn _osmosis_pow_approx(base: BigRational, exp: BigRational, precision: BigRational) -> BigRational {
    let zero: BigRational = BigRational::from_integer(0u128.into());
    let one: BigRational = BigRational::from_integer(1u128.into());
    if exp == zero {
        return one;
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
    let (x, x_neg) = _osmosis_abs_difference_with_sign(base, one.clone());
    let mut term = one.clone();
    let mut sum = one;
    let mut negative = false;

    // a := exp.Clone()
    // bigK := sdk.NewDec(0)
    let mut a = exp.clone();
    let mut big_k = zero.clone();

    // for i := int64(1); term.GTE(precision); i++ {
    let mut i: i64 = 0;
    loop {
        i += 1;
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
        big_k = BigRational::new_raw(i.into(), 1u128.into());
        term *= c * x.clone() / big_k.clone();

        // // a is mutated on absDifferenceWithSign, reset
        // a.Set(exp)

        // a is never mutated in our implementation. i think we can remove it and use exp directly.
        a = exp.clone();

        // if term.isZero() {
        //     break;
        // }
        if term == zero {
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
            sum -= term.clone();
        } else {
            sum += term.clone();
        }
    }
    return sum;
}

// AbsDifferenceWithSign returns | a - b |, (a - b).sign()
// a is mutated and returned.
fn _osmosis_abs_difference_with_sign(a: BigRational, b: BigRational) -> (BigRational, bool) {
    if a >= b {
        (a - b, false)
    } else {
        (b - a, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // // #[test_case(1, vec!["uosmo".to_string(), "uatom".to_string()], Decimal::from_ratio(1u8,50u8), Decimal::from_ratio(1u8,500u8), 1, 0.5;"test_join_pool_calculation_single_sided")]
    // // fn test_join_pool_calculation_single_sided(
    // //     num_accounts: u64,
    // //     pool_names: Vec<String>,
    // //     base: Decimal,
    // //     precision: Decimal,
    // //     exp: Decimal,
    // //     expected: Decimal,
    // // ) {
    // //     let actual = join_pool_calculation(num_accounts, pool_names, base, precision, exp, false);
    // //     assert_eq!(actual, expected);
    // // }

    #[derive(Clone)]
    struct CalcJoinSharesTestCase {
        pub name: String,
        pub swap_fee: Decimal,
        pub pool_assets: Vec<PoolAsset>,
        pub tokens_in: Vec<Coin>,
        pub expect_shares: Uint128,
    }

    #[derive(Clone)]
    struct PoolAsset {
        pub token: Coin,
        pub weight: Uint128,
    }

    #[test]
    fn test_osmosis_calculate_join_pool_shares_single_sided() {
        let one_trillion: u128 = 1e12 as u128;
        let default_osmo_pool_asset: PoolAsset = PoolAsset {
            token: Coin::new(one_trillion, "uosmo").into(),
            weight: Uint128::new(100),
        };
        let default_atom_pool_asset: PoolAsset = PoolAsset {
            token: Coin::new(one_trillion, "uatom").into(),
            weight: Uint128::new(100),
        };
        let one_trillion_even_pool_assets: Vec<PoolAsset> =
            vec![default_osmo_pool_asset.clone(), default_atom_pool_asset.clone()];

        let existing_pool_shares: Uint128 = Uint128::new(100_000_000_000_000_000_000);
        let calc_single_asset_join_test_cases: Vec<CalcJoinSharesTestCase> = vec![
        CalcJoinSharesTestCase {
            name:         "single tokens_in - equal weights with zero swap fee".to_string(),
            swap_fee:      Decimal::zero(),
            pool_assets:   one_trillion_even_pool_assets.clone(),
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(2_499_999_968_750),
        },
        CalcJoinSharesTestCase {
            name:         "single tokens_in - equal weights with 0.01 swap fee".to_string(),
            swap_fee:      Decimal::from_str("0.01").unwrap(),
            pool_assets:   one_trillion_even_pool_assets.clone(),
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(2_487_500_000_000),
        },
        CalcJoinSharesTestCase {
            name:         "single tokens_in - equal weights with 0.99 swap fee".to_string(),
            swap_fee:      Decimal::from_str("0.99").unwrap(),
            pool_assets:   one_trillion_even_pool_assets.clone(),
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(1_262_500_000_000),
        },
        CalcJoinSharesTestCase {
            name:    "single tokens_in - unequal weights with 0.99 swap fee".to_string(),
            swap_fee: Decimal::from_str("0.99").unwrap(),
            pool_assets: vec![
                default_osmo_pool_asset.clone(),
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(300),
                },
            ],
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(321_875_000_000),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - token in weight is greater than the other token, with zero swap fee".to_string(),
            swap_fee: Decimal::zero(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(one_trillion, "uosmo"),
                    weight: Uint128::new(500),
                },
                default_atom_pool_asset.clone(),
            ],
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(4_166_666_649_306),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - token in weight is greater than the other token, with non-zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0.01").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(one_trillion, "uosmo"),
                    weight: Uint128::new(500),
                },
                default_atom_pool_asset.clone(),
            ],
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(4_159_722_200_000),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - token in weight is smaller than the other token, with zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(one_trillion, "uosmo"),
                    weight: Uint128::new(200),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(1000),
                },
            ],
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(833_333_315_972),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - token in weight is smaller than the other token, with non-zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0.02").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(one_trillion, "uosmo"),
                    weight: Uint128::new(200),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(1000),
                },
            ],
            tokens_in:     vec![Coin::new(50_000, "uosmo")],
            expect_shares: Uint128::new(819_444_430_000),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - tokenIn is large relative to liquidity, token in weight is smaller than the other token, with zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(156_736, "uosmo"),
                    weight: Uint128::new(200),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(1000),
                },
            ],
            // 156_736 * 3 / 4 = 117552
            tokens_in: vec![Coin::new(117552, "uosmo")],
            expect_shares: Uint128::new(9_775_731_930_496_140_648),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - tokenIn is large relative to liquidity, token in weight is smaller than the other token, with non-zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0.02").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(156_736, "uosmo"),
                    weight: Uint128::new(200),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(1000),
                },
            ],
            // 156_736 / 4 * 3 = 117552
            tokens_in: vec![Coin::new(117552, "uosmo")],
            expect_shares: Uint128::new(9_644_655_900_000_000_000),
        },
        CalcJoinSharesTestCase {
            name:    "single asset - (almost 1 == tokenIn / liquidity ratio), token in weight is smaller than the other token, with zero swap fee".to_string(),
            swap_fee: Decimal::from_str("0").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(500_000, "uosmo"),
                    weight: Uint128::new(100),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(1000),
                },
            ],
            tokens_in: vec![Coin::new(499_999, "uosmo")],
            expect_shares: Uint128::new(6_504_099_261_800_144_638),
        },
        // TODO: Handle error and panic cases
        // CalcJoinSharesTestCase {
        //     // Currently, our Pow approximation function does not work correctly when one tries
        //     // to add liquidity that is larger than the existing liquidity.
        //     // The ratio of tokenIn / existing liquidity that is larger than or equal to 1 causes a panic.
        //     // This has been deemed as acceptable since it causes code complexity to fix
        //     // & only affects UX in an edge case (user has to split up single asset joins)
        //     name:    "single asset - (exactly 1 == tokenIn / liquidity ratio - failure), token in weight is smaller than the other token, with zero swap fee".to_string(),
        //     swap_fee: Decimal::from_str("0").unwrap(),
        //     pool_assets: vec![
        //         PoolAsset {
        //             token:  Coin::new(500_000, "uosmo"),
        //             weight: Uint128::new(100),
        //         },
        //         PoolAsset {
        //             token:  Coin::new(one_trillion, "uatom"),
        //             weight: Uint128::new(1000),
        //         },
        //     ],
        //     tokens_in: vec![Coin::new(500_000, "uosmo")],
        //     expect_shares: Uint128::new(6_504_099_261_800_144_638),
        //     expectPanic:  true,
        // },
        // CalcJoinSharesTestCase {
        //     name:         "tokenIn asset does not exist in pool",
        //     swap_fee:      Decimal::from_str("0"),
        //     pool_assets:   one_trillion_even_pool_assets,
        //     tokens_in:     vec![](Uint128::new64Coin(doesNotExistDenom, 50_000)),
        //     expect_shares: sdk.ZeroInt(),
        //     expErr:       sdkerrors.Wrapf(types.ErrDenomNotFoundInPool, fmt.Sprintf(balancer.ErrMsgFormatNoPoolAssetFound, doesNotExistDenom)),
        // },
        CalcJoinSharesTestCase {
            // Pool liquidity is changed by 1e-12 / 2
            // P_issued = 1e20 * 1e-12 / 2 = 1e8 / 2 = 50_000_000
            name:    "minimum input single asset equal liquidity".to_string(),
            swap_fee: Decimal::from_str("0").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(one_trillion, "uosmo"),
                    weight: Uint128::new(100),
                },
                PoolAsset {
                    token:  Coin::new(one_trillion, "uatom"),
                    weight: Uint128::new(100),
                },
            ],
            tokens_in: vec![Coin::new(1, "uosmo")],
            expect_shares: Uint128::new(50_000_000),
        },
        CalcJoinSharesTestCase {
            // P_issued should be 1/10th that of the previous test
            // p_issued = 50_000_000 / 10 = 5_000_000
            name:    "minimum input single asset imbalanced liquidity".to_string(),
            swap_fee: Decimal::from_str("0").unwrap(),
            pool_assets: vec![
                PoolAsset {
                    token:  Coin::new(10_000_000_000_000, "uosmo"),
                    weight: Uint128::new(100),
                },
                PoolAsset {
                    token:  Coin::new(1_000_000_000_000, "uatom"),
                    weight: Uint128::new(100),
                },
            ],
            tokens_in: vec![Coin::new(1, "uosmo")],
            expect_shares: Uint128::new(5_000_000),
        }];

        for test_case in calc_single_asset_join_test_cases {
            let token_in = test_case.tokens_in[0].clone();

            // Get the PoolAsset for the provided asset
            let provided_asset_pool =
                test_case.pool_assets.iter().find(|a| a.token.denom == token_in.denom).unwrap();

            // Calculate the normalized weight for the provided asset
            let total_weight: Uint128 = test_case.pool_assets.iter().map(|a| a.weight).sum();
            let normalized_weight = Decimal::from_ratio(provided_asset_pool.weight, total_weight);

            // Call function to calc single sided joining
            let actual = calc_pool_shares_out_given_single_asset_in(
                &test_case.tokens_in[0],
                existing_pool_shares,
                provided_asset_pool.token.amount,
                normalized_weight,
                test_case.swap_fee,
            )
            .unwrap();

            println!("Running test for Test case: {}", test_case.name);

            assert_eq!(actual, test_case.expect_shares);
        }
    }
}
