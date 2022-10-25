use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, Env, Event, QueryRequest, ReplyOn, Response, StdResult,
    SubMsg, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

use cw20_stake::{
    msg::{
        ExecuteMsg as Cw20StakeExecuteMsg, GetHooksResponse, QueryMsg as Cw20StakeQueryMsg,
        ReceiveMsg as Cw20StakeReceiveMsg,
    },
    state::Config,
};
use cw_utils::Duration;

use crate::{
    traits::{LockedStaking, Rewards, Stake, Unlock, Unstake},
    CwDexError,
};
use stake_cw20_external_rewards::msg::ExecuteMsg as StakeCw20ExternalRewardsExecuteMsg;

#[cw_serde]
pub struct JunoswapStaking {
    pub addr: Addr,
    pub lp_token_addr: Addr,
}

impl Stake for JunoswapStaking {
    fn stake(&self, deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let cfg = deps.querier.query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
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
        // Verify that the vault does not have an unbonding period. Our design assumes
        // that the vault does not have an unbonding period when unstake can be called.
        let cfg = deps.querier.query::<Config>(&QueryRequest::Wasm(WasmQuery::Smart {
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
    fn claim_rewards(&self, deps: Deps, _env: &Env) -> Result<Response, CwDexError> {
        let claim_messages = deps
            .querier
            .query::<GetHooksResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.addr.to_string(),
                msg: to_binary(&Cw20StakeQueryMsg::GetHooks {})?,
            }))?
            .hooks
            .iter()
            .map(|addr| {
                // Call as SubMsg since we don't know if the contracts in the hooks are
                // always going to be reward contracts. If not the messages for that
                // contract should fail without failing the transaction.
                Ok(SubMsg {
                    id: 0,
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: addr.to_string(),
                        funds: vec![],
                        msg: to_binary(&StakeCw20ExternalRewardsExecuteMsg::Claim {})?,
                    }),
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                })
            })
            .collect::<StdResult<Vec<SubMsg>>>()?;

        let event = Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "junoswap");

        Ok(Response::new().add_submessages(claim_messages).add_event(event))
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

        Ok(Response::new().add_message(claim_unlocked_msg).add_event(event))
    }

    fn unlock(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20StakeExecuteMsg::Unstake {
                amount,
            })?,
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
            .unstaking_duration;

        Ok(match duration {
            Some(duration) => match duration {
                cw_utils_0_13_4::Duration::Height(height) => Duration::Height(height),
                cw_utils_0_13_4::Duration::Time(time) => Duration::Time(time),
            },
            None => Duration::Time(0),
        })
    }
}
