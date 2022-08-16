use cosmwasm_std::{Deps, StdError, StdResult, Uint128};
use cw_asset::{Asset, AssetList, AssetListUnchecked};
use osmo_bindings::{OsmosisQuerier, OsmosisQuery};

pub fn get_join_pool_shares(
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
