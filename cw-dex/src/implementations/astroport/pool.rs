//! Pool trait implementation for Astroport

use std::str::FromStr;

use apollo_utils::iterators::IntoElementwise;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, Env, Event, QuerierWrapper,
    QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use cw_utils::Expiration;

use super::helpers::{
    adjust_precision, compute_current_amp, compute_d, query_pair_config, query_supply,
    query_token_precision, MAX_ALLOWED_SLIPPAGE, N_COINS, U256,
};
use super::msg::{
    PairCw20HookMsg, PairExecuteMsg, PairInfo, PairQueryMsg, PairType, PoolResponse,
    SimulationResponse,
};
use crate::traits::Pool;
use crate::CwDexError;
use apollo_utils::assets::separate_natives_and_cw20s;
use cw_asset::astroport::AstroAssetInfo;

/// Represents an AMM pool on Astroport
#[cw_serde]
pub struct AstroportPool {
    /// The address of the associated pair contract
    pub pair_addr: Addr,
    /// The address of the associated LP token contract
    pub lp_token_addr: Addr,
    /// The assets of the pool
    pub pool_assets: Vec<AssetInfo>,
    /// The type of pool represented: Constant product (*Xyk*) or *Stableswap*
    pub pair_type: PairType,
}

impl AstroportPool {
    /// Creates a new instance of `AstroportPool`
    ///
    /// Arguments:
    /// - `pair_addr`: The address of the pair contract associated with the pool
    pub fn new(deps: Deps, pair_addr: Addr) -> StdResult<Self> {
        let pair_info = deps
            .querier
            .query_wasm_smart::<PairInfo>(pair_addr.clone(), &PairQueryMsg::Pair {})?;

        // Validate pair type. We only support XYK and stable swap pools
        match pair_info.pair_type {
            PairType::Custom(_) => Err(StdError::generic_err("Custom pair type is not supported")),
            _ => Ok(()),
        }?;

        Ok(Self {
            pair_addr,
            lp_token_addr: pair_info.liquidity_token,
            pool_assets: pair_info.asset_infos.into_elementwise(),
            pair_type: pair_info.pair_type,
        })
    }

    /// Returns the total supply of the associated LP token
    pub fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, self.lp_token_addr.to_owned())
    }

    /// Queries the pair contract for the current pool state
    pub fn query_pool_info(&self, querier: &QuerierWrapper) -> StdResult<PoolResponse> {
        querier.query::<PoolResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&PairQueryMsg::Pool {})?,
        }))
    }

    /// Math for LP shares calculation when providing liquidity to an Astroport
    /// constant product pool.
    ///
    /// Copied from the astroport XYK pool implementation here:
    /// https://github.com/astroport-fi/astroport-core/blob/7bedc6f27e59ef8b921a0980be9bc30c4aab7459/contracts/pair/src/contract.rs#L297-L434
    fn xyk_simulate_provide_liquidity(
        &self,
        deps: Deps,
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let PoolResponse {
            assets: pools,
            total_share,
        } = self.query_pool_info(&deps.querier)?;

        let deposits = [
            assets
                .find(&pools[0].info.clone().into())
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            assets
                .find(&pools[1].info.clone().into())
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() || deposits[1].is_zero() {
            return Err(CwDexError::InvalidZeroAmount {});
        };

        let share = if total_share.is_zero() {
            // Initial share = collateral amount
            Uint128::new(
                (U256::from(deposits[0].u128()) * U256::from(deposits[1].u128()))
                    .integer_sqrt()
                    .as_u128(),
            )
        } else {
            // min(1, 2)
            // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share /
            // sqrt(pool_0 * pool_1)) == deposit_0 * total_share / pool_0
            // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share /
            // sqrt(pool_1 * pool_1)) == deposit_1 * total_share / pool_1
            std::cmp::min(
                deposits[0].multiply_ratio(total_share, pools[0].amount),
                deposits[1].multiply_ratio(total_share, pools[1].amount),
            )
        };

        let lp_token = Asset {
            info: AssetInfo::Cw20(self.lp_token_addr.clone()),
            amount: share,
        };
        Ok(lp_token)
    }

    /// Math for providing liquidity to an Astroport stable swap pool.
    ///
    /// This logic is copied from the astroport implementation here:
    /// https://github.com/astroport-fi/astroport-core/blob/f1caf2e4cba74d60ff0e8ae3abba9d9e1f88c06e/contracts/pair_stable/src/contract.rs#L338-L501
    fn stable_simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let config = query_pair_config(&deps.querier, self.pair_addr.clone())?;
        let mut pools = config
            .pair_info
            .query_pools(&deps.querier, self.pair_addr.to_owned())?;
        let deposits: [Uint128; 2] = [
            assets
                .find(&pools[0].info.clone().into())
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            assets
                .find(&pools[1].info.clone().into())
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
        ];

        if deposits[0].is_zero() && deposits[1].is_zero() {
            return Err(CwDexError::InvalidZeroAmount {});
        }

        for (i, pool) in pools.iter_mut().enumerate() {
            // we cannot put a zero amount into an empty pool.
            if deposits[i].is_zero() && pool.amount.is_zero() {
                return Err(CwDexError::InvalidProvideLPsWithSingleToken {});
            }
        }

        let token_precision_0 = query_token_precision(&deps.querier, pools[0].info.clone())?;
        let token_precision_1 = query_token_precision(&deps.querier, pools[1].info.clone())?;

        let greater_precision = token_precision_0.max(token_precision_1);

        let deposit_amount_0 = adjust_precision(deposits[0], token_precision_0, greater_precision)?;
        let deposit_amount_1 = adjust_precision(deposits[1], token_precision_1, greater_precision)?;

        let total_share = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;
        let share = if total_share.is_zero() {
            let liquidity_token_precision = query_token_precision(
                &deps.querier,
                AstroAssetInfo::Token {
                    contract_addr: config.pair_info.liquidity_token,
                },
            )?;

            // Initial share = collateral amount
            adjust_precision(
                Uint128::new(
                    (U256::from(deposit_amount_0.u128()) * U256::from(deposit_amount_1.u128()))
                        .integer_sqrt()
                        .as_u128(),
                ),
                greater_precision,
                liquidity_token_precision,
            )?
        } else {
            let leverage = compute_current_amp(&config, env)?
                .checked_mul(u64::from(N_COINS))
                .unwrap();

            let mut pool_amount_0 =
                adjust_precision(pools[0].amount, token_precision_0, greater_precision)?;
            let mut pool_amount_1 =
                adjust_precision(pools[1].amount, token_precision_1, greater_precision)?;

            let d_before_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            pool_amount_0 = pool_amount_0.checked_add(deposit_amount_0)?;
            pool_amount_1 = pool_amount_1.checked_add(deposit_amount_1)?;

            let d_after_addition_liquidity =
                compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

            // d after adding liquidity may be less than or equal to d before adding
            // liquidity because of rounding
            if d_before_addition_liquidity >= d_after_addition_liquidity {
                return Err(CwDexError::LiquidityAmountTooSmall {});
            }

            total_share.multiply_ratio(
                d_after_addition_liquidity - d_before_addition_liquidity,
                d_before_addition_liquidity,
            )
        };

        if share.is_zero() {
            return Err(CwDexError::Std(StdError::generic_err(
                "Insufficient amount of liquidity",
            )));
        }

        let lp_token = Asset {
            info: AssetInfoBase::Cw20(Addr::unchecked(self.lp_token_addr.to_string())),
            amount: share,
        };
        Ok(lp_token)
    }
}

impl Pool for AstroportPool {
    fn provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let lp_out = self.simulate_provide_liquidity(deps, env, assets.clone())?;
        if min_out > lp_out.amount {
            return Err(CwDexError::MinOutNotReceived {
                min_out,
                received: lp_out.amount,
            });
        }

        let msg = PairExecuteMsg::ProvideLiquidity {
            assets: assets.to_owned().try_into()?,
            slippage_tolerance: Some(Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?),
            auto_stake: Some(false),
            receiver: None,
        };

        let (funds, cw20s) = separate_natives_and_cw20s(&assets);

        // Increase allowance on all Cw20s
        let allowance_msgs: Vec<CosmosMsg> = cw20s
            .into_iter()
            .map(|asset| {
                Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: asset.address,
                    msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: self.pair_addr.to_string(),
                        amount: asset.amount,
                        expires: Some(Expiration::AtHeight(env.block.height + 1)),
                    })?,
                    funds: vec![],
                }))
            })
            .collect::<StdResult<Vec<_>>>()?;

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&msg)?,
            funds,
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("assets", format!("{:?}", assets));

        Ok(Response::new()
            .add_messages(allowance_msgs)
            .add_message(provide_liquidity)
            .add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        asset: Asset,
    ) -> Result<Response, CwDexError> {
        if let AssetInfoBase::Cw20(token_addr) = &asset.info {
            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&PairCw20HookMsg::WithdrawLiquidity {})?,
                })?,
                funds: vec![],
            });

            let event = Event::new("apollo/cw-dex/withdraw_liquidity")
                .add_attribute("pair_addr", &self.pair_addr)
                .add_attribute("asset", format!("{:?}", asset))
                .add_attribute("token_amount", asset.amount);

            Ok(Response::new()
                .add_message(withdraw_liquidity)
                .add_event(event))
        } else {
            Err(CwDexError::InvalidInAsset { a: asset })
        }
    }

    fn swap(
        &self,
        _deps: Deps,
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        // Setting belief price to the minimium acceptable return and max spread to zero
        // simplifies things Astroport will make the best possible swap that
        // returns at least `min_out`.
        let belief_price = Some(Decimal::from_ratio(offer_asset.amount, min_out));
        let swap_msg = match &offer_asset.info {
            AssetInfo::Native(_) => {
                let asset = offer_asset.clone().into();
                wasm_execute(
                    self.pair_addr.to_string(),
                    &PairExecuteMsg::Swap {
                        offer_asset: asset,
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    },
                    vec![offer_asset.clone().try_into()?],
                )
            }
            AssetInfo::Cw20(addr) => wasm_execute(
                addr.to_string(),
                &Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&PairCw20HookMsg::Swap {
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    })?,
                },
                vec![],
            ),
        }?;
        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("ask_asset", format!("{:?}", ask_asset_info))
            .add_attribute("offer_asset", format!("{:?}", offer_asset.info))
            .add_attribute("minimum_out_amount", min_out);
        Ok(Response::new().add_message(swap_msg).add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let resp = self.query_pool_info(&deps.querier)?;
        Ok(resp.assets.to_vec().into())
    }

    fn simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        match self.pair_type {
            PairType::Xyk {} => self.xyk_simulate_provide_liquidity(deps, env, assets),
            PairType::Stable {} => self.stable_simulate_provide_liquidity(deps, env, assets),
            PairType::Custom(_) => Err(CwDexError::Std(StdError::generic_err(
                "custom pair type not supported",
            ))),
        }
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError> {
        let amount = lp_token.amount;
        let total_share = self.query_lp_token_supply(&deps.querier)?;
        let mut share_ratio = Decimal::zero();
        if !total_share.is_zero() {
            share_ratio = Decimal::from_ratio(amount, total_share);
        }

        let pools = self.query_pool_info(&deps.querier)?.assets;
        Ok(pools
            .iter()
            .map(|a| Asset {
                info: a.info.clone().into(),
                amount: a.amount * share_ratio,
            })
            .collect::<Vec<Asset>>()
            .into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        _ask_asset_info: AssetInfo,
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        Ok(deps
            .querier
            .query::<SimulationResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.pair_addr.to_string(),
                msg: to_binary(&PairQueryMsg::Simulation {
                    offer_asset: offer_asset.into(),
                })?,
            }))?
            .return_amount)
    }

    fn lp_token(&self) -> AssetInfo {
        AssetInfoBase::Cw20(self.lp_token_addr.clone())
    }

    fn pool_assets(&self, _deps: Deps) -> StdResult<Vec<AssetInfo>> {
        Ok(self.pool_assets.clone())
    }
}
