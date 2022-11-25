mod pool_test {
    use cosmwasm_std::{testing::{mock_dependencies, mock_env}, Deps, Env};
    use cw_asset::AssetList;
    use test_case::test_case;

    use crate::{Pool, osmosis::OsmosisPool};

    #[test]
    fn provide_liquidity_test() {

        let deps: Deps=mock_dependencies();
        let env: Env = mock_env();
        let pool_id=1;
        let osmosis_pool: OsmosisPool = OsmosisPool::new(pool_id);
        let pool: Pool = Pool::Osmosis(osmosis_pool);

        let assets: AssetList = AssetList::new();

        pool.as_trait().provide_liquidity(deps, &env, assets, min_out);
    }
}