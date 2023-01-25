//! Staking/rewards traits implementations for Junoswap

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, Env, Event, QuerierWrapper, QueryRequest, Response, Uint128,
    WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

use cw_asset::AssetList;
use cw_utils::Duration;
use stake_cw20::msg::{
    ExecuteMsg as Cw20StakeExecuteMsg, QueryMsg as Cw20StakeQueryMsg,
    ReceiveMsg as Cw20StakeReceiveMsg,
};
use stake_cw20::state::Config;

use crate::traits::{LockedStaking, Rewards, Stake, Unlock, Unstake};
use crate::CwDexError;
// use stake_cw20_external_rewards::msg::{
//     ExecuteMsg as StakeCw20ExternalRewardsExecuteMsg, PendingRewardsResponse,
//     QueryMsg as StakeCw20ExternalRewardsQueryMsg,
// };

/// Represents staking of LP tokens on Junoswap
#[cw_serde]
pub struct JunoswapStaking {
    /// Address of the staking contract
    pub addr: Addr,
    /// Address of the LP token contract
    pub lp_token_addr: Addr,
}

impl Stake for JunoswapStaking {
    fn stake(&self, deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let cfg = deps
            .querier
            .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.addr.to_string(),
                msg: to_binary(&Cw20StakeQueryMsg::GetConfig {})?,
            }))?;

        let stake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.token_address.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: self.addr.to_string(),
                amount,
                msg: to_binary(&Cw20StakeReceiveMsg::Stake {})?,
            })?,
        });

        let event = Event::new("cw-dex/staking/stake")
            .add_attribute("type", "junoswap")
            .add_attribute("amount", amount.to_string());

        Ok(Response::new().add_message(stake_msg).add_event(event))
    }
}

impl Unstake for JunoswapStaking {
    fn unstake(&self, deps: Deps, env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        // Verify that the staking contract does not have an unbonding period.
        // Our design assumes that the vault does not have an unbonding period
        // when unstake can be called.
        let cfg = deps
            .querier
            .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.addr.to_string(),
                msg: to_binary(&Cw20StakeQueryMsg::GetConfig {})?,
            }))?;

        if cfg.unstaking_duration.is_some() {
            return Err(CwDexError::UnstakingDurationNotSupported {});
        }

        // Locked and non locked staking uses the same unstake message on Junoswap.
        self.unlock(deps, env, amount)
    }
}

impl Rewards for JunoswapStaking {
    fn claim_rewards(&self, _deps: Deps, _env: &Env) -> Result<Response, CwDexError> {
        todo!("Implement JunoswapStaking::claim_rewards")
        // let claim_messages = deps
        //     .querier
        //     .query::<GetHooksResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
        //         contract_addr: self.addr.to_string(),
        //         msg: to_binary(&Cw20StakeQueryMsg::GetHooks {})?,
        //     }))?
        //     .hooks
        //     .iter()
        //     .map(|addr| {
        //         // Call as SubMsg since we don't know if the contracts in the
        // hooks are         // always going to be reward contracts. If
        // not the messages for that         // contract should fail
        // without failing the transaction.         Ok(SubMsg {
        //             id: 0,
        //             msg: CosmosMsg::Wasm(WasmMsg::Execute {
        //                 contract_addr: addr.to_string(),
        //                 funds: vec![],
        //                 msg:
        // to_binary(&StakeCw20ExternalRewardsExecuteMsg::Claim {})?,
        //             }),
        //             gas_limit: None,
        //             reply_on: ReplyOn::Error,
        //         })
        //     })
        //     .collect::<StdResult<Vec<SubMsg>>>()?;
        //
        // let event =
        // Event::new("apollo/cw-dex/claim_rewards").add_attribute("type",
        // "junoswap");
        //
        // Ok(Response::new().add_submessages(claim_messages).add_event(event))
    }

    fn query_pending_rewards(
        &self,
        _querier: &QuerierWrapper,
        _user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        todo!("Implement JunoswapStaking::query_pending_rewards")
        // let hooks = querier
        //     .query::<GetHooksResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
        //         contract_addr: self.addr.to_string(),
        //         msg: to_binary(&Cw20StakeQueryMsg::GetHooks {})?,
        //     }))?
        //     .hooks;
        //
        // let mut assets = AssetList::new();
        // for hook in hooks {
        //     // Since we can't be sure that the hook is actually a reward
        // contract we must     // do .ok() and match only on `Some`
        // values to avoid failing the entire query.
        //     let pending_rewards = querier
        //         .query::<PendingRewardsResponse>(&
        // QueryRequest::Wasm(WasmQuery::Smart {
        // contract_addr: hook.to_string(),             msg:
        // to_binary(&StakeCw20ExternalRewardsQueryMsg::GetPendingRewards {
        //                 address: user.to_string(),
        //             })?,
        //         }))
        //         .ok();
        //
        //     if let Some(pending_rewards) = pending_rewards {
        //         let asset_info = match pending_rewards.denom {
        //             Denom::Native(x) => AssetInfo::Native(x),
        //             Denom::Cw20(x) => AssetInfo::Cw20(x),
        //         };
        //
        //         assets.add(&Asset::new(asset_info,
        // pending_rewards.pending_rewards))?;     }
        // }
        //
        // Ok(assets)
    }
}

impl Unlock for JunoswapStaking {
    fn withdraw_unlocked(
        &self,
        _deps: Deps,
        _env: &Env,
        amount: Uint128,
    ) -> Result<Response, CwDexError> {
        let claim_unlocked_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20StakeExecuteMsg::Claim {})?,
        });

        let event = Event::new("cw-dex/lockup/withdraw_unlocked")
            .add_attribute("type", "junoswap")
            .add_attribute("amount", amount.to_string());

        Ok(Response::new()
            .add_message(claim_unlocked_msg)
            .add_event(event))
    }

    fn unlock(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20StakeExecuteMsg::Unstake { amount })?,
        });

        let event = Event::new("cw-dex/staking/unstake")
            .add_attribute("type", "junoswap")
            .add_attribute("amount", amount.to_string());

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }
}

impl LockedStaking for JunoswapStaking {
    fn get_lockup_duration(&self, deps: Deps) -> Result<Duration, CwDexError> {
        let duration = deps
            .querier
            .query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.addr.to_string(),
                msg: to_binary(&Cw20StakeQueryMsg::GetConfig {})?,
            }))?
            .unstaking_duration
            .unwrap_or(cw_utils_0_11::Duration::Time(0));

        let duration = match duration {
            cw_utils_0_11::Duration::Time(x) => Duration::Time(x),
            cw_utils_0_11::Duration::Height(x) => Duration::Height(x),
        };

        Ok(duration)
    }
}
