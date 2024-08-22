//! Staking/rewards traits implementations for Astroport
use apollo_cw_asset::{AssetInfo, AssetList};
use apollo_utils::assets::separate_natives_and_cw20s;
use astroport::asset::Asset as AstroAsset;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coins, to_json_binary, Addr, CosmosMsg, Deps, Empty, Env, Event, QuerierWrapper, QueryRequest,
    Response, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;
use cw_dex::traits::{Rewards, Stake, Staking, Unstake};
use cw_dex::CwDexError;

use crate::pool::astroport_v5_vec_asset_to_assetlist;

/// Represents staking of tokens on Astroport
#[cw_serde]
pub struct AstroportStaking {
    /// The cw20 or token factory denom of the associated LP token
    pub lp_token: AssetInfo,
    /// The address of the astroport incentives contract
    pub incentives: Addr,
}

impl Staking for AstroportStaking {}

impl Stake for AstroportStaking {
    fn stake(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let stake_msg = match &self.lp_token {
            AssetInfo::Cw20(cw20_addr) => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_addr.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: self.incentives.to_string(),
                    amount,
                    msg: to_json_binary(&astroport::incentives::Cw20Msg::Deposit {
                        recipient: None,
                    })?,
                })?,
                funds: vec![],
            }),
            AssetInfo::Native(denom) => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: self.incentives.to_string(),
                msg: to_json_binary(&astroport::incentives::ExecuteMsg::Deposit {
                    recipient: None,
                })?,
                funds: coins(amount.into(), denom),
            }),
        };

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "astroport_staking")
            .add_attribute("asset", self.lp_token.to_string())
            .add_attribute("incentives contract address", self.incentives.to_string());

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
            contract_addr: self.incentives.to_string(),
            msg: to_json_binary(&astroport::incentives::ExecuteMsg::ClaimRewards {
                lp_tokens: vec![self.lp_token.to_string()],
            })?,
            funds: vec![],
        });

        let mut res = Response::new().add_message(claim_rewards_msg);

        // Astroport generator only supports CW20 tokens as proxy rewards and wraps
        // native tokens in their "CW20 wrapper". We need to unwrap them here.
        let (_, cw20s) = separate_natives_and_cw20s(&claimable_rewards);
        for cw20 in cw20s {
            // Query the cw20s creator to get the address of the wrapper contract
            let contract_info = deps.querier.query_wasm_contract_info(&cw20.address)?;
            let wrapper_contract = deps.api.addr_validate(&contract_info.creator)?;

            // Query the wrapper contract's cw2 info to check if it is a native token
            // wrapper, otherwise skip it
            let contract_version = cw2::query_contract_info(&deps.querier, &wrapper_contract).ok();
            match contract_version {
                Some(contract_version) => {
                    if &contract_version.contract != "astroport-native-coin-wrapper" {
                        continue;
                    }
                }
                None => continue,
            }

            // Unwrap the native token
            let unwrap_msg: CosmosMsg<Empty> = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20.address.to_string(),
                msg: to_json_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: wrapper_contract.to_string(),
                    amount: cw20.amount,
                    msg: to_json_binary(
                        &astroport_v2::native_coin_wrapper::Cw20HookMsg::Unwrap {},
                    )?,
                })?,
                funds: vec![],
            });
            res = res.add_message(unwrap_msg);
        }

        Ok(res.add_event(event))
    }

    fn query_pending_rewards(
        &self,
        querier: &QuerierWrapper,
        user: &Addr,
    ) -> Result<AssetList, CwDexError> {
        let pending_rewards: Vec<AstroAsset> = querier
            .query::<Vec<AstroAsset>>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.incentives.to_string(),
                msg: to_json_binary(&astroport::incentives::QueryMsg::PendingRewards {
                    lp_token: self.lp_token.to_string(),
                    user: user.to_string(),
                })?,
            }))?
            .into_iter()
            .filter(|asset| !asset.amount.is_zero()) //TODO: Is this necessary?
            .collect();

        Ok(astroport_v5_vec_asset_to_assetlist(pending_rewards))
    }
}

impl Unstake for AstroportStaking {
    fn unstake(&self, _deps: Deps, _env: &Env, amount: Uint128) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.incentives.to_string(),
            msg: to_json_binary(&astroport::incentives::ExecuteMsg::Withdraw {
                lp_token: self.lp_token.to_string(),
                amount,
            })?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/unstake").add_attribute("type", "astroport_staking");

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }
}
