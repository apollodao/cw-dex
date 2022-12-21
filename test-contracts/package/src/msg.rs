use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{to_binary, Coin, CosmosMsg, Uint128, WasmMsg};
use cw_asset::{Asset, AssetInfo, AssetList};

#[cw_serde]
pub struct InstantiateMsg {
    pub pool_id: u64,
    pub lock_duration: u64,
    pub lock_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    ProvideLiquidity {
        assets: AssetList,
        min_out: Uint128,
    },
    WithdrawLiquidity {
        amount: Uint128,
    },
    Stake {
        amount: Uint128,
    },
    Unlock {
        amount: Uint128,
    },
    WithdrawUnlocked {
        amount: Uint128,
    },
    ForceUnlock {
        amount: Uint128,
        lockup_id: u64,
    },
    Swap {
        offer: Asset,
        ask: AssetInfo,
        min_out: Uint128,
    },
}

impl ExecuteMsg {
    pub fn into_cosmos_msg(&self, contract_addr: String, funds: Vec<Coin>) -> CosmosMsg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: to_binary(self).unwrap(),
            funds,
        })
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(AssetList)]
    PoolLiquidity {},
    #[returns(Uint128)]
    SimulateProvideLiquidity { assets: AssetList },
    #[returns(Uint128)]
    SimulateSwap {
        offer: Asset,
        ask: AssetInfo,
        sender: Option<String>,
    },
}
