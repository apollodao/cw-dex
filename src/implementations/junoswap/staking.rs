use apollo_utils::{response_prefix, with_dollar_sign};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, Env, QuerierWrapper, QueryRequest, ReplyOn, Response,
    StdResult, SubMsg, Uint128, WasmMsg, WasmQuery,
};
use cw20::{Cw20ExecuteMsg, Denom};

use cw20_stake::{
    msg::{
        ExecuteMsg as Cw20StakeExecuteMsg, GetHooksResponse, QueryMsg as Cw20StakeQueryMsg,
        ReceiveMsg as Cw20StakeReceiveMsg,
    },
    state::Config,
};
use cw_asset::{Asset, AssetInfo, AssetList};
use cw_utils::Duration;

use crate::{
    traits::{LockedStaking, Rewards, Stake, Unlock, Unstake},
    CwDexError,
};
use stake_cw20_external_rewards::msg::{
    ExecuteMsg as StakeCw20ExternalRewardsExecuteMsg, PendingRewardsResponse,
    QueryMsg as StakeCw20ExternalRewardsQueryMsg,
};

response_prefix!("apollo/cw-dex/junoswap");
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

        Ok(response!(
            "stake",
            [("type", "junoswap_staking"), ("amount", amount.to_string())],
            [stake_msg]
        ))
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
                    reply_on: ReplyOn::Error,
                })
            })
            .collect::<StdResult<Vec<SubMsg>>>()?;

        Ok(response!("claim_rewards", [("type", "junoswap_staking")])
            .add_submessages(claim_messages))
    }

    fn query_pending_rewards(
        &self,
        querier: &QuerierWrapper,
        user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        let hooks = querier
            .query::<GetHooksResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.addr.to_string(),
                msg: to_binary(&Cw20StakeQueryMsg::GetHooks {})?,
            }))?
            .hooks;

        let mut assets = AssetList::new();
        for hook in hooks {
            // Since we can't be sure that the hook is actually a reward contract we must
            // do .ok() and match only on `Some` values to avoid failing the entire query.
            let pending_rewards = querier
                .query::<PendingRewardsResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: hook.to_string(),
                    msg: to_binary(&StakeCw20ExternalRewardsQueryMsg::GetPendingRewards {
                        address: user.to_string(),
                    })?,
                }))
                .ok();

            if let Some(pending_rewards) = pending_rewards {
                let asset_info = match pending_rewards.denom {
                    Denom::Native(x) => AssetInfo::Native(x),
                    Denom::Cw20(x) => AssetInfo::Cw20(x),
                };

                assets.add(&Asset::new(asset_info, pending_rewards.pending_rewards))?;
            }
        }

        Ok(assets)
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

        Ok(response!(
            "withdraw_unlocked",
            [("type", "junoswap_staking"), ("amount", amount.to_string())],
            [claim_unlocked_msg]
        ))
    }

    fn unlock(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20StakeExecuteMsg::Unstake {
                amount,
            })?,
        });

        Ok(response!(
            "unstake",
            [("type", "junoswap_staking"), ("amount", amount.to_string())],
            [unstake_msg]
        ))
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

        Ok(duration.unwrap_or(Duration::Time(0)))
    }
}
