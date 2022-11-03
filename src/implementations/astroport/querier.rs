use astroport_core::asset::{AssetInfo, PairInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_slice, Addr, Binary, CanonicalAddr, Decimal, Empty, QuerierWrapper, QueryRequest,
    StdResult, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;

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

// needed to simualate provide liquidity
pub fn query_asset_precision(
    querier: &QuerierWrapper,
    pair: &Addr,
    asset: AssetInfo,
) -> StdResult<u8> {
    if let Some(res) = querier.query_wasm_raw(
        pair,
        Binary::from(concat(&to_length_prefixed(b"precisions"), asset.to_string().as_bytes())),
    )? {
        let precision: u8 = from_slice(&res)?;
        Ok(precision)
    } else {
        return Err(cosmwasm_std::StdError::GenericErr {
            msg: format!("Raw query failed: precision for {} not found", asset).to_string(),
        });
    }
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
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
