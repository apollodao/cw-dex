use cosmwasm_std::{Coin, Decimal, Deps, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetList, AssetListUnchecked};
use osmo_bindings::{OsmosisQuerier, OsmosisQuery};

pub fn get_join_pool_shares_osmosis(
    deps: Deps<OsmosisQuery>,
    pool_id: u64,
    assets: AssetList,
) -> StdResult<Uint128> {
    let osmosis_querier = OsmosisQuerier::new(&deps.querier);
    let pool_state = osmosis_querier.query_pool_state(pool_id)?;

    if assets.len() == 1 {
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
            return Ok(shares_out_est_1);
        }
    }

    Ok(Uint128::zero())
}

pub fn get_exit_pool_amounts_osmosis(
    deps: Deps<OsmosisQuery>,
    pool_id: u64,
    exit_share_amount: Uint128,
    exit_fee: Decimal, // TODO: queriable?
) -> StdResult<Vec<Coin>> {
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
