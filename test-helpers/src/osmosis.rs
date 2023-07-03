use apollo_cw_asset::AssetList;
use cosmwasm_std::{Addr, Coin, Uint128};
use cw_dex_test_contract::msg::OsmosisTestContractInstantiateMsg;
use cw_it::helpers::upload_wasm_file;
use cw_it::osmosis::OsmosisTestPool;
use cw_it::osmosis_test_tube::{
    Module, OsmosisTestApp, Runner, RunnerResult, SigningAccount, Wasm,
};
use cw_it::{Artifact, ContractType};

/// Setup a pool and test contract for testing.
pub fn setup_pool_and_test_contract(
    pool: &OsmosisTestPool,
    lock_id: u64,
    lock_duration: Option<u64>,
    superfluid_validator_addr: Option<Addr>,
    wasm_file_path: &str,
) -> RunnerResult<(OsmosisTestApp, Vec<SigningAccount>, u64, String)> {
    let runner = OsmosisTestApp::new();

    // Initialize 10 accounts with max balance of each token in the pool
    let accs = runner
        .init_accounts(
            &pool
                .liquidity
                .iter()
                .chain(vec![Coin::new(u128::MAX, "uosmo".to_string())].iter())
                .map(|c| Coin::new(u128::MAX, c.denom.clone()))
                .collect::<Vec<_>>(),
            10,
        )
        .unwrap();

    // Create pool
    let pool_id = pool.create(&runner, &accs[0]);

    // Upload test contract wasm file
    let code_id = upload_wasm_file(
        &runner,
        &accs[0],
        ContractType::Artifact(Artifact::Local(wasm_file_path.to_string())),
    )
    .unwrap();

    // Instantiate the test contract
    let contract_addr = instantiate_test_contract(
        &runner,
        code_id,
        pool_id,
        lock_id,
        lock_duration,
        superfluid_validator_addr,
        &accs[0],
    )?;

    Ok((runner, accs, pool_id, contract_addr))
}

/// Create an [`AssetList`] from a slice of tuples of asset denominations and
/// amounts.
pub fn native_assetlist_from_slice(assets: &[(&str, Uint128)]) -> AssetList {
    assets
        .iter()
        .map(|(denom, amount)| Coin {
            denom: denom.to_string(),
            amount: *amount,
        })
        .collect::<Vec<_>>()
        .into()
}

pub fn instantiate_test_contract<'a, R: Runner<'a>>(
    runner: &'a R,
    code_id: u64,
    pool_id: u64,
    lock_id: u64,
    lock_duration: Option<u64>,
    superfluid_validator_addr: Option<Addr>,
    signer: &SigningAccount,
) -> RunnerResult<String> {
    let init_msg = OsmosisTestContractInstantiateMsg {
        pool_id,
        lock_duration,
        lock_id,
        superfluid_validator: superfluid_validator_addr,
    };

    let wasm = Wasm::new(runner);
    Ok(wasm
        .instantiate(code_id, &init_msg, None, None, &[], signer)?
        .data
        .address)
}
