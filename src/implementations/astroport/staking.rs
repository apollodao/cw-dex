//! Staking/rewards traits implementations for Astroport

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, Env, Event, QuerierWrapper, QueryRequest, Response, Uint128,
    WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

use super::msg::{
    GeneratorCw20HookMsg, GeneratorExecuteMsg, GeneratorQueryMsg, PendingTokenResponse,
};
use cw_asset::astroport::AstroAsset;
use cw_asset::{Asset, AssetList};

use crate::traits::{Rewards, Stake, Staking, Unstake};
use crate::CwDexError;

/// Represents staking of tokens on Astroport
#[cw_serde]
pub struct AstroportStaking {
    /// The address of the associated LP token contract
    pub lp_token_addr: Addr,
    /// The address of the associated generator contract
    pub generator_addr: Addr,
    /// The address of the ASTRO token contract
    pub astro_addr: Addr,
}

impl Staking for AstroportStaking {}

impl Stake for AstroportStaking {
    fn stake(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let stake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.lp_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: self.generator_addr.to_string(),
                amount,
                msg: to_binary(&GeneratorCw20HookMsg::Deposit {})?,
            })?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "astroport_staking")
            .add_attribute("asset", self.lp_token_addr.to_string())
            .add_attribute("generator_address", self.generator_addr.to_string());

        Ok(Response::new().add_message(stake_msg).add_event(event))
    }
}

impl Rewards for AstroportStaking {
    fn claim_rewards(&self, deps: Deps, env: &Env) -> Result<Response, CwDexError> {
        let claimable_rewards: AssetList =
            self.query_pending_rewards(&deps.querier, &env.contract.address)?;

        let event =
            Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "astroport_staking");

        if claimable_rewards.len() == 0 {
            return Ok(Response::new().add_event(event));
        }

        let claim_rewards_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.generator_addr.to_string(),
            msg: to_binary(&GeneratorExecuteMsg::ClaimRewards {
                lp_tokens: vec![self.lp_token_addr.to_string()],
            })?,
            funds: vec![],
        });

        Ok(Response::new()
            .add_message(claim_rewards_msg)
            .add_event(event))
    }

    fn query_pending_rewards(
        &self,
        querier: &QuerierWrapper,
        user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        let PendingTokenResponse {
            pending: pending_astro,
            pending_on_proxy,
        } = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.generator_addr.to_string(),
            msg: to_binary(&GeneratorQueryMsg::PendingToken {
                lp_token: self.lp_token_addr.to_string(),
                user: user.to_string(),
            })?,
        }))?;

        let pending_rewards: Vec<AstroAsset> = pending_on_proxy
            .unwrap_or_default()
            .into_iter()
            .chain(vec![
                Asset::cw20(self.astro_addr.clone(), pending_astro).into()
            ])
            .filter(|asset| !asset.amount.is_zero())
            .collect::<Vec<_>>();

        Ok(pending_rewards.into())
    }
}

impl Unstake for AstroportStaking {
    fn unstake(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.generator_addr.to_string(),
            msg: to_binary(&GeneratorExecuteMsg::Withdraw {
                lp_token: self.lp_token_addr.to_string(),
                amount,
            })?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/unstake").add_attribute("type", "astroport_staking");

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }
}
