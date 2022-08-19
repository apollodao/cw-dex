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
        let pool_amount_out = solve_constant_function_invariant(
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

        let shares_in = solve_constant_function_invariant(
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

pub(crate) fn vec_into<A, B: Into<A>>(v: Vec<B>) -> Vec<A> {
    v.into_iter().map(|x| x.into()).collect()
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
/// even weight of the two assets.
fn solve_constant_function_invariant(
    token_balance_fixed_before: Uint128,
    token_balance_fixed_after: Uint128,
    token_weight_fixed: Decimal,
    token_balance_unknown_before: Uint128,
    token_weight_unknown: Decimal,
    exiting: bool,
) -> StdResult<Uint128> {
    if token_weight_fixed != Decimal::from_ratio(1u128, 2u128) {
        return Err(StdError::generic_err(
            "token weight fixed must be 0.5 for single sided entry and exit",
        ));
    }

    // let y = token_balance_fixed_before.checked_div(token_balance_fixed_after)?;
    let y = Decimal::from_ratio(token_balance_fixed_before, token_balance_fixed_after);
    // let y_to_weight_ratio = y.pow(weight_ratio);

    // Workaround that only works for dual pools with even weight of the two assets.
    // Gets around the issue of the weight ratio being negative.
    // weight_ratio = token_weight_fixed - token_weight_unknown
    // For an evenly weighted dual pool the token_weight_fixed is 0.5. When exiting the pool
    // this function is called with token_weight_unknown = 1 => weight_ratio = 0.5 - 1 = -0.5.
    // When entering the pool this function is called with token_weight_unknown = -1 => weight_ratio = 0.5 - (-1) = 1.5.
    let y_to_weight_ratio = if exiting {
        Decimal::one() / y.sqrt() // = y ^ weight_ratio (when weight_ratio = -0.5)
    } else {
        y * y.sqrt() // = y ^ weight_ratio (when weight_ratio = 1.5)
    };

    let paranthetical: Decimal = Decimal::one() - Decimal::new(y_to_weight_ratio);
    let amount_y = token_balance_unknown_before * paranthetical;
    Ok(amount_y)
}
