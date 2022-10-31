use astroport_core::querier::query_supply;
use crate::error::CwDexError;
use astroport_core::generator::{
    Cw20HookMsg as GeneratorCw20HookMsg, ExecuteMsg as GeneratorExecuteMsg,
};
use astroport_core::querier::query_supply;
use astroport_core::querier::{query_fee_info, query_token_precision};
use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Querier, QuerierWrapper, Response, StdResult, WasmMsg,
};
use cosmwasm_std::{Decimal256, Env};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_asset::{Asset, AssetInfo, AssetList, AssetListBase};

use astroport_core::asset::{Asset as AstroAsset, AssetInfo as AstroAssetInfo};
use astroport_core::pair::{Cw20HookMsg, ExecuteMsg as PairExecMsg};
use cw20::Cw20ExecuteMsg;
use astroport_core::asset::{AssetInfo as AstroAssetInfo, DecimalAsset};
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use itertools::Itertools;

use astroport_core::asset::{
    Asset as AstroAsset, Decimal256Ext, PairInfo, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport_core::pair::{
    Cw20HookMsg, ExecuteMsg as PairExecMsg, PoolResponse, QueryMsg, SimulationResponse,
};

use crate::astroport::helpers::{compute_current_amp, compute_d};
use crate::astroport::querier::query_pair_config;
use crate::pool::Pool;
use crate::CwDexError;

pub struct AstroportPool {
    contract_addr: String,
    lp_token_addr: String,
}

impl AstroportPool {
    fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, &self.lp_token_addr)
    }

    fn query_asset_supply(
        &self,
        querier: &QuerierWrapper,
        asset_info: &AstroAssetInfo,
    ) -> StdResult<Uint128> {
        asset_info.query_pool(querier, &self.contract_addr)
    }
}

impl Pool for AstroportPool {
    fn provide_liquidity(
        &self,
        _deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let mut astro_assets: Vec<AstroAsset> = vec![];
        for asset_base in assets.into_iter() {
            let info = match &asset_base.info {
                AssetInfoBase::Native(denom) => Ok(AstroAssetInfo::NativeToken {
                    denom: denom.to_string(),
                }),
                AssetInfoBase::Cw20(addr) => Ok(AstroAssetInfo::Token {
                    contract_addr: addr.to_owned(),
                }),
                AssetInfoBase::Cw1155(addr, _) => Ok(AstroAssetInfo::Token {
                    contract_addr: addr.to_owned(),
                }),
                x => Err(CwDexError::InvalidInAsset {
                    a: asset_base.to_owned(),
                }),
            }?;
            let amount = asset_base.amount;
            astro_assets.push(AstroAsset {
                info,
                amount,
            })
        }
        let msg = PairExecMsg::ProvideLiquidity {
            assets: astro_assets.clone(),
            slippage_tolerance,
            auto_stake: Some(false), // Should this be true?
            receiver: Some(recipient.to_string()),
        };
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("factory_addr", &self.factory_addr)
            .add_attribute("assets", format!("{:?}", astro_assets)) // This one is maybe unnecessary
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let astro_asset = cw_asset_to_astro_asset(&asset)?;
        // Withdraw asset from pool
        let hook_msg = Cw20HookMsg::WithdrawLiquidity {
            assets: vec![astro_asset.to_owned()],
        };
        let recv_msg = Cw20ReceiveMsg {
            sender: recipient.to_string(),
            amount: asset.amount,
            msg: to_binary(&hook_msg)?,
        };

        // Calculate amount of LP tokens to send, corresponding to withdrawal amount
        let total_asset_supply = self.query_asset_supply(&deps.querier, &astro_asset.info)?;
        let share_ratio = Decimal::from_ratio(asset.amount, total_asset_supply);
        let total_token_supply = self.query_lp_token_supply(&deps.querier)?;
        let token_amount = share_ratio * total_token_supply;
        // Send to pool (pair)
        let exec_msg = Cw20ExecuteMsg::Send {
            contract: self.contract_addr.to_string(),
            amount: token_amount,
            msg: to_binary(&recv_msg)?,
        };

        // Execute on LP token contract
        let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.lp_token_addr.to_string(),
            msg: to_binary(&exec_msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/withdraw_liquidity")
            .add_attribute("pair_addr", &self.contract_addr)
            .add_attribute("asset", format!("{:?}", asset))
            .add_attribute("token_amount", token_amount)
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(withdraw_liquidity).add_event(event))
    }

    fn swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        // @todo check if asset is native or token
        // @todo calculate belief price
        todo!()
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        todo!()
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: Env,
        asset: AssetList,
    ) -> Result<Asset, CwDexError> {
        todo!()
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        env: Env,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        todo!()
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        //For some reason Osmosis requires us to send a sender address for simulation.
        //This obviously makes no sense and I guess we'll have to make a PR to
        //Osmosis to fix this, or perhaps copy their math and perform the calculation here...
        sender: Option<String>,
    ) -> StdResult<Uint128> {
        todo!()
    }
}

#[cw_serde]
pub struct AstroportStableSwapPool {
    contract_addr: String,
    lp_token_addr: String,
    generator_addr: String,
}

impl AstroportStableSwapPool {
    fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, &self.lp_token_addr)
    }

    fn swap_native_msg(
        &self,
        offer_asset: &Asset,
        ask_asset_info: &AssetInfo,
        belief_price: Decimal,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        let asset = cw_asset_to_astro_asset(offer_asset)?;
        let msg = PairExecMsg::Swap {
            offer_asset: asset,
            ask_asset_info: Some(cw_asset_info_to_astro_asset_info(ask_asset_info)?),
            belief_price: Some(belief_price),
            max_spread: Some(Decimal::zero()),
            to: Some(recipient.into_string()),
        };
        let swap = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.contract_addr.to_owned(),
            msg: to_binary(&msg)?,
            funds: vec![offer_asset.try_into()?],
        });
        Ok(Response::new().add_message(swap))
    }

    fn swap_cw20_msg(
        &self,
        offer_asset: &Asset,
        ask_asset_info: &AssetInfo,
        belief_price: Decimal,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = offer_asset.to_owned().info {
            let swap = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.contract_addr.to_string(),
                    amount: Uint128::zero(), // Should this be `offer_asset.amount`?
                    msg: to_binary(&Cw20HookMsg::Swap {
                        ask_asset_info: Some(cw_asset_info_to_astro_asset_info(ask_asset_info)?),
                        belief_price: Some(belief_price),
                        max_spread: Some(Decimal::zero()),
                        to: Some(recipient.into_string()),
                    })?,
                })?,
                funds: vec![],
            });
            Ok(Response::new().add_message(swap))
        } else {
            Err(CwDexError::InvalidInAsset {
                a: offer_asset.to_owned(),
            })
        }
    }

    fn get_pool_liquidity_impl(&self, querier: &QuerierWrapper) -> StdResult<PoolResponse> {
        let query_msg = QueryMsg::Pool {};
        let wasm_query = WasmQuery::Smart {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&query_msg)?,
        };
        let query_request = QueryRequest::Wasm(wasm_query);
        querier.query::<PoolResponse>(&query_request)
    }
}

impl Pool for AstroportStableSwapPool {
    fn provide_liquidity(
        &self,
        _deps: Deps,
        assets: AssetList,
        recipient: Addr,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let astro_assets: Vec<AstroAsset> = AstroAssetList::try_from(assets)?.into();

        let msg = PairExecMsg::ProvideLiquidity {
            assets: astro_assets.clone(),
            slippage_tolerance,
            auto_stake: Some(false), // Should this be true?
            receiver: Some(recipient.to_string()),
        };
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pair_addr", &self.contract_addr)
            .add_attribute("assets", format!("{:?}", astro_assets))
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        asset: Asset,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = &asset.info {
            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.contract_addr.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {
                        assets: vec![],
                    })?,
                })?,
                funds: vec![],
            });

            let event = Event::new("apollo/cw-dex/withdraw_liquidity")
                .add_attribute("pair_addr", &self.contract_addr)
                .add_attribute("asset", format!("{:?}", asset))
                .add_attribute("token_amount", asset.amount)
                .add_attribute("recipient", recipient.to_string());

            Ok(Response::new().add_message(withdraw_liquidity).add_event(event))
        } else {
            Err(CwDexError::InvalidInAsset {
                a: asset,
            })
        }
    }

    fn swap(
        &self,
        _deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
        recipient: Addr,
    ) -> Result<Response, CwDexError> {
        // Setting belief price to the minimium acceptable return and max spread to zero simplifies things
        // Astroport will make the best possible swap that returns at least minimum_out_amount
        let belief_price = Decimal::from_ratio(minimum_out_amount, 1u128);
        let response = match offer_asset.info {
            AssetInfo::Native(_) => {
                self.swap_native_msg(&offer_asset, &ask_asset_info, belief_price, recipient)
            }
            AssetInfo::Cw20(_) => {
                self.swap_cw20_msg(&offer_asset, &ask_asset_info, belief_price, recipient)
            }
            _ => Err(CwDexError::InvalidInAsset {
                a: offer_asset.clone(),
            }),
        }?;
        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pair_addr", &self.contract_addr)
            .add_attribute("ask_asset", format!("{:?}", ask_asset_info))
            .add_attribute("offer_asset", format!("{:?}", offer_asset.info))
            .add_attribute("minimum_out_amount", minimum_out_amount);
        Ok(response.add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let resp = self.get_pool_liquidity_impl(&deps.querier)?;
        Ok(AssetList::from(AstroAssetList(resp.assets)))
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let astro_assets: AstroAssetList = assets.try_into()?;
        let mut pools = vec![];
        for asset in astro_assets.0.clone() {
            pools.push(AstroStableSwapAsset {
                info: asset.info.clone(),
                amount: asset.info.query_pool(&deps.querier, self.contract_addr.to_string())?,
                precision: query_token_precision(&deps.querier, &asset.info)?,
            })
        }

        let deposits = [
            astro_assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[0].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            astro_assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[1].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() || deposits[1].is_zero() {
            return Err(StdError::generic_err("Either asset cannot be zero").into());
        };

        let mut non_zero_flag = false;

        let assets_collection = assets_collection
        .iter()
        .cloned()
        .map(|(asset, pool)| {
            let coin_precision = query_token_precision(&deps.querier, &asset.info)?;
            Ok((
                asset.to_decimal_asset(coin_precision)?,
                Decimal256::with_precision(pool, coin_precision)?,
            ))
        })
        .collect::<StdResult<Vec<(DecimalAsset, Decimal256)>>>()?;

        let config = query_pair_config(&deps.querier, &Addr::unchecked(self.contract_addr))?;

        let amp = compute_current_amp(deps, &env, config)?;

        // Initial invariant (D)
        let old_balances = pools.iter().map(|pool| pool.amount).collect_vec();
        let init_d = compute_d(amp, &old_balances, config.greatest_precision)?;

        for (i, pool) in pools.iter_mut().enumerate() {
            pool.amount =
                pool.amount.checked_sub(deposits[i]).map_err(|_| CwDexError::BigIntOverflow {})?;
        }
        let old_balances = assets_collection.iter().map(|(_, pool)| *pool).collect_vec();

        let deposit_d = compute_d(amp, &new_balances, config.greatest_precision)?;

        let pair_info: PairInfo =
            deps.querier.query_wasm_smart(self.contract_addr, &QueryMsg::Pair {})?;

        let n_coins = pair_info.asset_infos.len() as u8;

        // Initial invariant (D)
        let old_balances = assets_collection.iter().map(|(_, pool)| *pool).collect_vec();
        let init_d = compute_d(amp, &old_balances, config.greatest_precision)?;

        let deposit_d = compute_d(amp, &new_balances, config.greatest_precision)?;

        let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
        let share = if total_share.is_zero() {
            let share = deposit_d
                .to_uint128_with_precision(config.greatest_precision)?
                .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
                .map_err(|_| StdError::generic_err("Either asset cannot be zero"))?;

            // share cannot become zero after minimum liquidity subtraction
            if share.is_zero() {
                return Err(error::CwDexError::Std(StdError::generic_err(
                    "Either asset cannot be zero",
                )));
            }

            share
        } else {
            // Get fee info from the factory
            let fee_info = query_fee_info(
                &deps.querier,
                &config.factory_addr,
                config.pair_info.pair_type.clone(),
            )?;

            // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
            let fee = fee_info
                .total_fee_rate
                .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

            let fee = Decimal256::new(fee.atomics().into());

            for i in 0..n_coins as usize {
                let ideal_balance = deposit_d.checked_multiply_ratio(old_balances[i], init_d)?;
                let difference = if ideal_balance > new_balances[i] {
                    ideal_balance - new_balances[i]
                } else {
                    new_balances[i] - ideal_balance
                };
                // Fee will be charged only during imbalanced provide i.e. if invariant D was changed
                new_balances[i] -= fee.checked_mul(difference)?;
            }

            let after_fee_d = compute_d(amp, &new_balances, config.greatest_precision)?;

            let share = Decimal256::with_precision(total_share, config.greatest_precision)?
                .checked_multiply_ratio(after_fee_d.saturating_sub(init_d), init_d)?
                .to_uint128_with_precision(config.greatest_precision)?;

            if share.is_zero() {
                return Err(ContractError::LiquidityAmountTooSmall {});
            }

            share
        };

        let lp_token = Asset {
            info: AssetInfoBase::Cw20(Addr::unchecked(self.lp_token_addr.to_string())),
            amount: share,
        };
        Ok(lp_token)
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        asset: Asset,
    ) -> Result<AssetList, CwDexError> {
        let amount = asset.amount;
        let total_share = self.query_lp_token_supply(&deps.querier)?;
        let mut share_ratio = Decimal::zero();
        if !total_share.is_zero() {
            share_ratio = Decimal::from_ratio(amount, total_share);
        }

        let pools = self.get_pool_liquidity_impl(&deps.querier)?.assets;
        Ok(pools
            .iter()
            .map(|a| Asset {
                info: astro_asset_info_to_cw_asset_info(&a.info),
                amount: a.amount * share_ratio,
            })
            .collect::<Vec<Asset>>()
            .into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        _minimum_out_amount: Uint128, // Astroport doesn't allow setting max spread or belief price on simulated swaps
        _sender: Option<String>,      // N/A for Astroport
    ) -> StdResult<Uint128> {
        let query_msg = QueryMsg::Simulation {
            offer_asset: cw_asset_to_astro_asset(&offer_asset)?,
            ask_asset_info: Some(cw_asset_info_to_astro_asset_info(&ask_asset_info)?),
        };
        let wasm_query = WasmQuery::Smart {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&query_msg)?,
        };
        let query_request = QueryRequest::Wasm(wasm_query);
        let resp = deps.querier.query::<SimulationResponse>(&query_request)?;
        Ok(resp.return_amount)
    }
}

impl Staking for AstroportStableSwapPool {
    fn stake(&self, _deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let stake_msg = CosmosMsg::Wasm(
            (WasmMsg::Execute {
                contract_addr: self.lp_token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.generator_addr.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&GeneratorCw20HookMsg::Deposit {})?,
                })?,
                funds: vec![],
            }),
        );

        let event = Event::new("apollo/cw-dex/stake")
            .add_attribute("type", "astroport_staking")
            .add_attribute("asset", asset.to_string())
            .add_attribute("recipient", recipient.to_string())
            .add_attribute("generator_address", self.generator_addr.to_string());

        Ok(Response::new()
            .add_submessage(SubMsg {
                id: ASTROPORT_LOCK_TOKENS_REPLY_ID,
                msg: stake_msg,
                gas_limit: None,
                reply_on: ReplyOn::Success,
            })
            .add_event(event))
    }

    fn unstake(&self, _deps: Deps, asset: Asset, recipient: Addr) -> Result<Response, CwDexError> {
        let unstake_msg = CosmosMsg::Wasm(
            (WasmMsg::Execute {
                contract_addr: self.lp_token_addr.to_string(),
                msg: to_binary(&GeneratorExecuteMsg::Withdraw {
                    lp_token: self.lp_token_addr.to_string(),
                    amount: asset.amount,
                })?,
                funds: vec![],
            }),
        );

        let event = Event::new("apollo/cw-dex/unstake")
            .add_attribute("type", "astroport_staking")
            .add_attribute("recipient", recipient.to_string());

        Ok(Response::new().add_message(unstake_msg).add_event(event))
    }

    fn claim_rewards(&self, _recipient: Addr) -> Result<Response, CwDexError> {
        let claim_rewards_msg = CosmosMsg::Wasm(
            (WasmMsg::Execute {
                contract_addr: self.generator_addr.to_string(),
                msg: to_binary(&GeneratorExecuteMsg::ClaimRewards {
                    lp_tokens: vec![self.lp_token_addr.to_string()],
                })?,
                funds: vec![],
            }),
        );

        let event =
            Event::new("apollo/cw-dex/claim_rewards").add_attribute("type", "astroport_staking");
        Ok(Response::new().add_message(claim_rewards_msg).add_event(event))
    }
}

/// This enum describes a Terra asset (native or CW20).
#[cw_serde]
pub struct AstroStableSwapAsset {
    /// Information about an asset stored in a [`AssetInfo`] struct
    pub info: AstroAssetInfo,
    /// A token amount
    pub amount: Uint128,
    /// decimal precision
    pub precision: u8,
}
