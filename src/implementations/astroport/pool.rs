use astroport_core::asset::PairInfo;
use astroport_core::factory::PairType;
use astroport_core::querier::{query_supply, query_token_precision};
use astroport_core::U256;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Env, QuerierWrapper, QueryRequest, Response, StdError,
    StdResult, WasmMsg, WasmQuery,
};
use cosmwasm_std::{Deps, Event, Uint128};
use cw20::Cw20ExecuteMsg;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};

use astroport_core::asset::Asset as AstroAsset;
use astroport_core::asset::AssetInfo as AstroAssetInfo;
use astroport_core::pair::{
    Cw20HookMsg, ExecuteMsg as PairExecMsg, PoolResponse, QueryMsg, QueryMsg as PairQueryMsg,
    SimulationResponse,
};

use crate::traits::Pool;
use crate::CwDexError;

use super::helpers::{
    adjust_precision, astro_asset_info_to_cw_asset_info, compute_current_amp, compute_d,
    cw_asset_to_astro_asset, query_pair_config, AstroAssetList, N_COINS,
};

#[cw_serde]
pub struct AstroportPool {
    pub pair_addr: Addr,
    pub lp_token_addr: Addr,
    pub pair_type: PairType,
}

impl AstroportPool {
    pub fn new(deps: Deps, pair_addr: Addr) -> StdResult<Self> {
        let pair_info: PairInfo =
            deps.querier.query_wasm_smart(pair_addr.clone(), &PairQueryMsg::Pair {})?;

        // Validate pair type. We only support XYK and stable swap pools
        match pair_info.pair_type {
            PairType::Custom(_) => Err(StdError::generic_err("Custom pair type is not supported")),
            _ => Ok(()),
        }?;

        Ok(Self {
            pair_addr,
            lp_token_addr: pair_info.liquidity_token,
            pair_type: pair_info.pair_type,
        })
    }

    pub fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        query_supply(querier, self.lp_token_addr.to_owned())
    }

    fn get_pool_liquidity_impl(&self, querier: &QuerierWrapper) -> StdResult<PoolResponse> {
        querier.query::<PoolResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&QueryMsg::Pool {})?,
        }))
    }

    /// Math for LP shares calculation when providing liquidity to an Astroport constant product pool.
    ///
    /// Copied from the astroport XYK pool implementation here:
    /// https://github.com/astroport-fi/astroport-core/blob/7bedc6f27e59ef8b921a0980be9bc30c4aab7459/contracts/pair/src/contract.rs#L297-L434
    fn xyk_simulate_provide_liquidity(
        &self,
        deps: Deps,
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let astro_assets: AstroAssetList = assets.try_into()?;

        let PoolResponse {
            assets: pools,
            total_share,
        } = self.get_pool_liquidity_impl(&deps.querier)?;

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

        // map over pools
        const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
        let share = if total_share.is_zero() {
            // Initial share = collateral amount
            let share = Uint128::new(
                (U256::from(deposits[0].u128()) * U256::from(deposits[1].u128()))
                    .integer_sqrt()
                    .as_u128(),
            );

            if share.lt(&MINIMUM_LIQUIDITY_AMOUNT) {
                return Err(StdError::generic_err(
                    "Share cannot be less than minimum liquidity amount",
                )
                .into());
            }

            share
        } else {
            // Assert slippage tolerance
            // assert_slippage_tolerance(slippage_tolerance, &deposits, &pools)?;

            // min(1, 2)
            // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_1))
            // == deposit_0 * total_share / pool_0
            // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
            // == deposit_1 * total_share / pool_1
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
    /// https://github.com/astroport-fi/astroport-core/blob/c216ecd4f350113316be44d06a95569f451ac681/contracts/pair_stable/src/contract.rs#L338-L501
    fn stable_simulate_provide_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        let assets: AstroAssetList = assets.try_into()?;
        let config = query_pair_config(&deps.querier, self.pair_addr.clone())?;
        let mut pools: [AstroAsset; 2] =
            config.pair_info.query_pools(&deps.querier, self.pair_addr.to_owned())?;
        let deposits: [Uint128; 2] = [
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[0].info))
                .map(|a| a.amount)
                .expect("Wrong asset info is given"),
            assets
                .0
                .iter()
                .find(|a| a.info.equal(&pools[1].info))
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
            let leverage =
                compute_current_amp(&config, env)?.checked_mul(u64::from(N_COINS)).unwrap();

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

            // d after adding liquidity may be less than or equal to d before adding liquidity because of rounding
            if d_before_addition_liquidity >= d_after_addition_liquidity {
                return Err(CwDexError::LiquidityAmountTooSmall {});
            }

            total_share.multiply_ratio(
                d_after_addition_liquidity - d_before_addition_liquidity,
                d_before_addition_liquidity,
            )
        };

        if share.is_zero() {
            return Err(CwDexError::Std(StdError::generic_err("Insufficient amount of liquidity")));
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
        _deps: Deps,
        _env: &Env,
        assets: AssetList,
        slippage_tolerance: Option<Decimal>,
    ) -> Result<Response, CwDexError> {
        let astro_assets: AstroAssetList = assets.clone().try_into()?;

        let msg = PairExecMsg::ProvideLiquidity {
            assets: astro_assets
                .0
                .try_into()
                .map_err(|_| CwDexError::Std(StdError::generic_err("invalid assets for pair")))?,
            slippage_tolerance,
            auto_stake: Some(false),
            receiver: None,
        };
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.pair_addr.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        });

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("assets", format!("{:?}", assets));

        Ok(Response::new().add_message(provide_liquidity).add_event(event))
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
                    msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {})?,
                })?,
                funds: vec![],
            });

            let event = Event::new("apollo/cw-dex/withdraw_liquidity")
                .add_attribute("pair_addr", &self.pair_addr)
                .add_attribute("asset", format!("{:?}", asset))
                .add_attribute("token_amount", asset.amount);

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
        env: &Env,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        minimum_out_amount: Uint128,
    ) -> Result<Response, CwDexError> {
        // Setting belief price to the minimium acceptable return and max spread to zero simplifies things
        // Astroport will make the best possible swap that returns at least minimum_out_amount
        let belief_price = Some(Decimal::from_ratio(offer_asset.amount, minimum_out_amount));
        let swap_msg = match &offer_asset.info {
            AssetInfo::Native(_) => {
                let asset = cw_asset_to_astro_asset(&offer_asset)?;
                Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: self.pair_addr.to_string(),
                    msg: to_binary(&PairExecMsg::Swap {
                        offer_asset: asset,
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    })?,
                    funds: vec![offer_asset.clone().try_into()?],
                }))
            }
            AssetInfo::Cw20(addr) => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&Cw20HookMsg::Swap {
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                    })?,
                })?,
                funds: vec![],
            })),
            _ => Err(CwDexError::InvalidInAsset {
                a: offer_asset.clone(),
            }),
        }?;
        let event = Event::new("apollo/cw-dex/swap")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("ask_asset", format!("{:?}", ask_asset_info))
            .add_attribute("offer_asset", format!("{:?}", offer_asset.info))
            .add_attribute("minimum_out_amount", minimum_out_amount);
        Ok(Response::new().add_message(swap_msg).add_event(event))
    }

    fn get_pool_liquidity(&self, deps: Deps) -> Result<AssetList, CwDexError> {
        let resp = self.get_pool_liquidity_impl(&deps.querier)?;
        Ok(AssetList::from(AstroAssetList(resp.assets.to_vec())))
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
            PairType::Custom(_) => {
                Err(CwDexError::Std(StdError::generic_err("custom pair type not supported")))
            }
        }
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
        _ask_asset_info: AssetInfo,
        _sender: Option<String>,
    ) -> StdResult<Uint128> {
        Ok(deps
            .querier
            .query::<SimulationResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.pair_addr.to_string(),
                msg: to_binary(&QueryMsg::Simulation {
                    offer_asset: cw_asset_to_astro_asset(&offer_asset)?,
                })?,
            }))?
            .return_amount)
    }
}
