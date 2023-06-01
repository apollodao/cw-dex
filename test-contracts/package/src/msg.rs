use apollo_cw_asset::{Asset, AssetInfo, AssetList};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{to_binary, Addr, Coin, CosmosMsg, Uint128, WasmMsg};

#[cw_serde]
pub struct OsmosisTestContractInstantiateMsg {
    pub pool_id: u64,
    pub lock_duration: Option<u64>,
    pub lock_id: u64,
    pub superfluid_validator: Option<Addr>,
}

#[cw_serde]
pub struct AstroportContractInstantiateMsg {
    pub pair_addr: String,
    pub lp_token_addr: String,
    pub generator_addr: String,
    pub astro_addr: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ProvideLiquidity {
        assets: AssetList,
        min_out: Uint128,
    },
    WithdrawLiquidity {
        amount: Uint128,
        min_out: AssetList,
    },
    Stake {
        amount: Uint128,
    },
    Unlock {
        amount: Uint128,
    },
    SuperfluidStake {
        amount: Uint128,
    },
    SuperfluidUnlock {
        amount: Uint128,
    },
    WithdrawUnlocked {
        amount: Uint128,
    },
    ForceUnlock {
        amount: Uint128,
        lockup_id: Option<u64>,
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

#[cw_serde]
pub enum AstroportExecuteMsg {
    ProvideLiquidity {
        assets: AssetList,
        min_out: Uint128,
    },
    WithdrawLiquidity {
        amount: Uint128,
        min_out: AssetList,
    },
    Stake {
        amount: Uint128,
    },
    Unstake {
        amount: Uint128,
    },
    Swap {
        offer: Asset,
        ask: AssetInfo,
        min_out: Uint128,
    },
}

impl AstroportExecuteMsg {
    pub fn into_cosmos_msg(&self, contract_addr: String, funds: Vec<Coin>) -> CosmosMsg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: to_binary(self).unwrap(),
            funds,
        })
    }
}
