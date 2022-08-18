use std::{convert::TryInto, ops::Sub, str::FromStr};

use cosmwasm_std::{Coin, Decimal, Deps, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetList, AssetListUnchecked};
use osmo_bindings::{OsmosisQuerier, OsmosisQuery};

pub fn calculate_join_pool_shares_osmosis(
    deps: Deps<OsmosisQuery>,
    pool_id: u64,
    assets: AssetList,
    total_weight: Uint128,
    normalized_weight: Decimal,
    swap_fee: Decimal,
) -> StdResult<Coin> {
    let osmosis_querier = OsmosisQuerier::new(&deps.querier);
    let pool_state = osmosis_querier.query_pool_state(pool_id)?;

    if assets.len() == 1 {
        // deduct swapfee on the in asset.
        // We don't charge swap fee on the token amount that we imagine as unswapped (the normalized weight).
        // So effective_swapfee = swapfee * (1 - normalized_token_weight)
        // tokenAmountInAfterFee := tokenAmountIn.Mul(feeRatio(normalizedTokenWeightIn, swapFee))
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
        let token_in = &assets[0];
        let total_shares = pool_state.shares.amount;
        let provided_asset_1_pool_balance =
            pool_state.denom_pool_balance(&token_in.info.to_string());

        let token_in_amount_after_fee =
            token_in.amount * (Decimal::one() - normalized_weight).checked_mul(swap_fee)?;
        let pool_amount_out = osmosis_solve_constant_function_invariant(
            provided_asset_1_pool_balance.checked_add(token_in_amount_after_fee)?,
            provided_asset_1_pool_balance,
            normalized_weight,
            total_shares,
            //This will result in runtime error, need redo function
            Decimal::zero() - Decimal::one(),
        )?;
        return Ok(Coin {
            denom: token_in.info.to_string(),
            amount: pool_amount_out,
        });
        // Here we should add the calculation for JoinSwapExactAmountIN
    }
    if assets.len() == 2 {
        let provided_asset_1 = &assets[0];
        let provided_asset_2 = &assets[1];
        let provided_asset_1_pool_balance =
            pool_state.denom_pool_balance(&provided_asset_1.info.to_string());
        let provided_asset_2_pool_balance =
            pool_state.denom_pool_balance(&provided_asset_2.info.to_string());
        let total_shares = pool_state.shares.amount;
        let shares_out_est_1 = provided_asset_1
            .amount
            .checked_mul(total_shares)?
            .checked_div(provided_asset_1_pool_balance)?;

        let shares_out_est_2 = provided_asset_2
            .amount
            .checked_mul(total_shares)?
            .checked_div(provided_asset_2_pool_balance)?;

        //Different estimations will be returned if pool assets are not equal in value

        if shares_out_est_1 != shares_out_est_2 {
            return Err(StdError::generic_err("assets being added to pool must be equal in value"));
        } else {
            return Ok(Coin {
                denom: pool_state.shares.denom,
                amount: shares_out_est_1,
            });
        }
    }

    // TODO: Probably should remove this?
    Ok(Coin {
        denom: pool_state.shares.denom,
        amount: Uint128::zero(),
    })
}

/// Calculates the [[`Coin`]] amounts that will be returned when withdrawing `exit_share_amount` LP shares from the pool
/// with pool id `pool_id` on Osmosis. The implementation is a translation of the calculations performed in the Go code
/// of the GAMM module. See
/// https://github.com/osmosis-labs/osmosis/blob/91c7830d7d195aad53378d60b24224a67e70fd7f/x/gamm/pool-models/internal/cfmm_common/lp.go#L16
pub fn calculate_exit_pool_amounts_osmosis(
    deps: Deps<OsmosisQuery>,
    pool_id: u64,
    exit_share_amount: Uint128,
    exit_fee: Decimal, // TODO: queriable?
    swap_fee: Decimal,
    normalized_weight: Decimal,
    total_weight: Uint128,
    token_out: Option<Coin>,
) -> StdResult<Vec<Coin>> {
    // TODO: Remove go code comments after review
    let osmosis_querier = OsmosisQuerier::new(&deps.querier);
    let pool_state = osmosis_querier.query_pool_state(pool_id)?;

    // totalShares := pool.GetTotalShares()
    // if exitingShares.GTE(totalShares) {
    // 	return sdk.Coins{}, sdkerrors.Wrapf(types.ErrLimitMaxAmount, errMsgFormatSharesLargerThanMax, exitingShares, totalShares)
    // }

    let total_shares = pool_state.shares.amount;
    if exit_share_amount >= total_shares {
        return Err(StdError::generic_err("exit share amount must be less than total shares"));
    }

    if let Some(token_out) = token_out {
        if !pool_state.has_denom(&token_out.denom) {
            return Err(StdError::generic_err("request asset to withdraw is not in the pool"));
        }

        // tokenAmountOutFeeIncluded := tokenAmountOut.Quo(feeRatio(normalizedTokenWeightOut, swapFee))

        // // delta poolSupply is positive(total pool shares decreases)
        // // pool weight is always 1
        // sharesIn := solveConstantFunctionInvariant(tokenBalanceOut.Sub(tokenAmountOutFeeIncluded), tokenBalanceOut, normalizedTokenWeightOut, totalPoolSharesSupply, sdk.OneDec())

        // // charge exit fee on the pool token side
        // // pAi = pAiAfterExitFee/(1-exitFee)
        // sharesInFeeIncluded := sharesIn.Quo(sdk.OneDec().Sub(exitFee))

        let pool_asset_out = pool_state.denom_pool_balance(&token_out.denom);

        let token_amount_out_fee_included: Uint128 = Uint128::new(1)
            * (Decimal::new(token_out.amount)
                / (Decimal::one() - ((Decimal::one() - normalized_weight) * swap_fee)));

        let shares_in = osmosis_solve_constant_function_invariant(
            token_out.amount.checked_sub(token_amount_out_fee_included)?,
            token_out.amount,
            normalized_weight,
            total_shares,
            Decimal::one(),
        )?;

        let shares_in_fee_included =
            Uint128::new(1) * (Decimal::new(shares_in) / (Decimal::one() - exit_fee));

        if shares_in_fee_included > exit_share_amount {
            return Err(StdError::generic_err("too many shares out"));
        };

        return Ok(vec![Coin {
            denom: token_out.denom,
            amount: shares_in_fee_included,
        }]);
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
        refunded_shares =
            Decimal::from_ratio(exit_share_amount, 1u128).checked_mul(Decimal::one() - exit_fee)?;
    } else {
        refunded_shares = Decimal::from_ratio(exit_share_amount, 1u128);
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
) -> StdResult<Uint128> {
    // // weightRatio = (weightX/weightY)
    // weightRatio := tokenWeightFixed.Quo(tokenWeightUnknown)
    let weight_ratio = token_weight_fixed / token_weight_unknown;

    // // y = balanceXBefore/balanceXAfter
    // y := tokenBalanceFixedBefore.Quo(tokenBalanceFixedAfter)
    let y = Decimal::from_ratio(token_balance_fixed_before, token_balance_fixed_after);

    // // amountY = balanceY * (1 - (y ^ weightRatio))
    // yToWeightRatio := osmomath.Pow(y, weightRatio)
    // paranthetical := sdk.OneDec().Sub(yToWeightRatio)
    // amountY := tokenBalanceUnknownBefore.Mul(paranthetical)
    // return amountY
    let y_to_weight_ratio = osmosis_pow(y, weight_ratio)?;
    let paranthetical = Decimal::one() - y_to_weight_ratio;
    let amount_y = token_balance_unknown_before * paranthetical;
    return Ok(amount_y);
}

fn osmosis_pow(base: Decimal, exp: Decimal) -> StdResult<Decimal> {
    if base >= Decimal::from_ratio(2u128, 1u128) {
        return Err(StdError::generic_err("base must be lesser than two"));
    }

    // // We will use an approximation algorithm to compute the power.
    // // Since computing an integer power is easy, we split up the exponent into
    // // an integer component and a fractional component.
    // integer := exp.TruncateDec()
    // fractional := exp.Sub(integer)
    let integer = exp * Uint128::from(1u128);
    let fractional: Decimal = exp - Decimal::from_ratio(integer, 1u128);

    // integerPow := base.Power(uint64(integer.TruncateInt64()))
    let integer_pow = base.checked_pow(integer.u128() as u32)?;

    // if fractional.IsZero() {
    // 	return integerPow
    // }
    if fractional.is_zero() {
        return Ok(integer_pow);
    }

    // fractionalPow := PowApprox(base, fractional, powPrecision)
    let fractional_pow = osmosis_pow_approx(base, fractional, Decimal::from_ratio(1u128, 1u128));

    // return integerPow.Mul(fractionalPow)
    return Ok(integer_pow.checked_mul(fractional_pow)?);
}

// Contract: 0 < base <= 2
// 0 <= exp < 1.
fn osmosis_pow_approx(base: Decimal, exp: Decimal, precision: Decimal) -> Decimal {
    if exp.is_zero() {
        return Decimal::one();
    }

    // Common case optimization
    // Optimize for it being equal to one-half
    if exp == Decimal::from_ratio(1u128, 2u128) {
        return base.sqrt();
    }
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
    let (x, x_neg) = osmosis_abs_difference_with_sign(base, Decimal::one());
    let mut term = Decimal::one();
    let mut sum = Decimal::one();
    let mut negative = false;

    // a := exp.Clone()
    // bigK := sdk.NewDec(0)
    let mut a = exp.clone();
    let mut big_k = Decimal::zero();

    // for i := int64(1); term.GTE(precision); i++ {
    let mut i: i64 = 0;
    loop {
        i += 1;
        if term >= precision {
            break;
        }

        // // At each iteration, we need two values, i and i-1.
        // // To avoid expensive big.Int allocation, we reuse bigK variable.
        // // On this line, bigK == i-1.
        // c, cneg := AbsDifferenceWithSign(a, bigK)
        let (c, c_neg) = osmosis_abs_difference_with_sign(a, big_k);

        // // On this line, bigK == i.
        // bigK.Set(sdk.NewDec(i))
        // term.MulMut(c).MulMut(x).QuoMut(bigK)
        big_k = Decimal::from_ratio(i as u128, 1u128);
        term *= c * x / big_k;

        // // a is mutated on absDifferenceWithSign, reset
        // a.Set(exp)

        // a is never mutated in our implementation. i think we can remove it and use exp directly.
        a = exp;

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
            sum -= term;
        } else {
            sum += term;
        }
    }
    return sum;
}

// AbsDifferenceWithSign returns | a - b |, (a - b).sign()
// a is mutated and returned.
fn osmosis_abs_difference_with_sign(a: Decimal, b: Decimal) -> (Decimal, bool) {
    if a >= b {
        (a - b, false)
    } else {
        (b - a, true)
    }
}
