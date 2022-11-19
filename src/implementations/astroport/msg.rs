//! Contains a subset of the messages for Astroport Pair and Generator contracts
//! used by the Astroport implementation.
#![allow(missing_docs)]

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, QuerierWrapper, StdResult, Uint128};

use cw_asset::astroport::{AstroAsset, AstroAssetInfo};

///////////////////////////////
// Pair Msgs
///////////////////////////////

#[cw_serde]
pub enum PairType {
    Xyk {},
    Stable {},
    Custom(String),
}

#[cw_serde]
pub struct PairInfo {
    pub asset_infos: [AstroAssetInfo; 2],
    pub contract_addr: Addr,
    pub liquidity_token: Addr,
    pub pair_type: PairType,
}
impl PairInfo {
    pub fn query_pools(
        &self,
        querier: &QuerierWrapper,
        contract_addr: Addr,
    ) -> StdResult<[AstroAsset; 2]> {
        Ok([
            AstroAsset {
                amount: self.asset_infos[0].query_pool(querier, contract_addr.clone())?,
                info: self.asset_infos[0].clone(),
            },
            AstroAsset {
                amount: self.asset_infos[1].query_pool(querier, contract_addr)?,
                info: self.asset_infos[1].clone(),
            },
        ])
    }
}

#[cw_serde]
pub enum PairCw20HookMsg {
    /// Sell a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    WithdrawLiquidity {},
}

#[cw_serde]
pub enum PairExecuteMsg {
    /// ProvideLiquidity a user provides pool liquidity
    ProvideLiquidity {
        assets: [AstroAsset; 2],
        slippage_tolerance: Option<Decimal>,
        auto_stake: Option<bool>,
        receiver: Option<String>,
    },
    /// Swap an offer asset to the other
    Swap {
        offer_asset: AstroAsset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
}

#[cw_serde]
pub enum PairQueryMsg {
    Pair {},
    Pool {},
    Simulation { offer_asset: AstroAsset },
}

#[cw_serde]
pub struct PoolResponse {
    pub assets: [AstroAsset; 2],
    pub total_share: Uint128,
}

#[cw_serde]
pub struct SimulationResponse {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// Astroport stable pair config
#[cw_serde]
pub struct Config {
    pub pair_info: PairInfo,
    pub factory_addr: Addr,
    pub block_time_last: u64,
    pub price0_cumulative_last: Uint128,
    pub price1_cumulative_last: Uint128,
    pub init_amp: u64,
    pub init_amp_time: u64,
    pub next_amp: u64,
    pub next_amp_time: u64,
}

///////////////////////////////
// Generator Msgs
///////////////////////////////

#[cw_serde]
pub enum GeneratorCw20HookMsg {
    Deposit {},
    DepositFor(Addr),
}

#[cw_serde]
pub enum GeneratorExecuteMsg {
    ClaimRewards { lp_tokens: Vec<String> },
    Withdraw { lp_token: String, amount: Uint128 },
}

#[cw_serde]
pub enum GeneratorQueryMsg {
    PendingToken { lp_token: String, user: String },
}

#[cw_serde]
pub struct PendingTokenResponse {
    pub pending: Uint128,
    pub pending_on_proxy: Option<Vec<AstroAsset>>,
}

///////////////////////////////
// Factory Msgs
///////////////////////////////

#[cw_serde]
pub enum FactoryQueryMsg {
    Config {},
    Pair {
        asset_infos: [AstroAssetInfo; 2],
    },
    Pairs {
        start_after: Option<[AstroAssetInfo; 2]>,
        limit: Option<u32>,
    },
    FeeInfo {
        pair_type: PairType,
    },
}

#[cw_serde]
pub struct FeeInfoResponse {
    pub fee_address: Option<Addr>,
    pub total_fee_bps: u16,
    pub maker_fee_bps: u16,
}

#[cw_serde]
pub struct FeeInfo {
    pub fee_address: Option<Addr>,
    pub total_fee_rate: Decimal,
    pub maker_fee_rate: Decimal,
}
