#[cfg(test)]

mod tests {
    use crate::{implementations::osmosis::OsmosisPool, Pool};
    use cosmwasm_std::{Decimal, Uint128};
    use test_case::test_case;

    #[test_case(1, vec!["uosmo".to_string(), "uatom".to_string()], Decimal::from_ratio(1u8,50u8), Decimal::from_ratio(1u8,500u8), 1, 0.5;"test_join_pool_calculation_single_sided")]
    fn test_join_pool_calculation_single_sided(
        pool_id: u64,
        assets: Vec<String>,
        swap_fee: Decimal,
        exit_fee: Decimal,
        total_weight: Uint128,
        normalized_weight: Decimal,
    ) {
        let osmosis_pool: OsmosisPool = OsmosisPool {
            pool_id,
            assets,
            exit_fee,
            swap_fee,
            total_weight,
            normalized_weight,
        };

        osmosis_pool.simulate_provide_liquidity(deps, assets.into_iter()).unwrap();
    }
}
