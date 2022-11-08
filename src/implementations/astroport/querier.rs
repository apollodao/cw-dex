use astroport_core::asset::{AssetInfo, PairInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_slice, Addr, QuerierWrapper, StdResult, Uint128};

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

#[cw_serde]
pub struct Config {
    /// The contract owner
    pub owner: Option<Addr>,
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract update the asset cumulative prices
    pub block_time_last: u64,
    /// This is the current amplification used in the pool
    pub init_amp: u64,
    /// This is the start time when amplification starts to scale up or down
    pub init_amp_time: u64,
    /// This is the target amplification to reach at `next_amp_time`
    pub next_amp: u64,
    /// This is the timestamp when the current pool amplification should be `next_amp`
    pub next_amp_time: u64,
    /// The greatest precision of assets in the pool
    pub greatest_precision: u8,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
}
