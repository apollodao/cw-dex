//! Pool trait implementation for Astroport

use std::str::FromStr;

use apollo_cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use apollo_utils::iterators::IntoElementwise;
use astroport::liquidity_manager;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coins, to_json_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, Env, Event,
    QuerierWrapper, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;
use cw_utils::Expiration;

use apollo_utils::assets::separate_natives_and_cw20s;
use astroport::asset::Asset as AstroAsset;
use astroport::factory::PairType;
use astroport::pair::{
    Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, PoolResponse,
    QueryMsg as PairQueryMsg, SimulationResponse, MAX_ALLOWED_SLIPPAGE,
};
use astroport::querier::query_supply;
use cw_dex::traits::Pool;
use cw_dex::CwDexError;

/// Represents an AMM pool on Astroport
#[cw_serde]
pub struct AstroportPool {
    /// The address of the associated pair contract
    pub pair_addr: Addr,
    /// The address of the associated LP token contract
    pub lp_token: AssetInfo,
    /// The assets of the pool
    pub pool_assets: Vec<AssetInfo>,
    /// The type of pool represented: Constant product (*Xyk*) or *Stableswap*
    pub pair_type: PairType,
    /// The address of the Astroport liquidity manager contract
    pub liquidity_manager: Option<Addr>,
}

impl AstroportPool {
    /// Creates a new instance of `AstroportPool`
    ///
    /// Arguments:
    /// - `pair_addr`: The address of the pair contract associated with the pool
    pub fn new(deps: Deps, pair_addr: Addr, liquidity_manager: Option<Addr>) -> StdResult<Self> {
        let pair_info = deps
            .querier
            .query_wasm_smart::<astroport_v5::asset::PairInfo>(
                pair_addr.clone(),
                &PairQueryMsg::Pair {},
            )?;

        // Validate pair type. We only support XYK, stable swap, and PCL pools
        match &pair_info.pair_type {
            astroport_v5::factory::PairType::Custom(t) => match t.as_str() {
                "concentrated" => Ok(()),

                "astroport-pair-xyk-sale-tax" => Ok(()),
                _ => Err(StdError::generic_err("Custom pair type is not supported")),
            },
            _ => Ok(()),
        }?;

        let lp_token = AssetInfo::from_str(deps.api, &pair_info.liquidity_token);

        // Only allow using liquidity manager if LP token is a CW20 token
        if lp_token.is_native() && liquidity_manager.is_some() {
            return Err(StdError::generic_err(
                "Liquidity manager is not supported for native LP tokens",
            ));
        }
        // Require liquidity manager to be set if LP token is a CW20 token
        if !lp_token.is_native() && liquidity_manager.is_none() {
            return Err(StdError::generic_err(
                "Liquidity manager must be set for CW20 LP tokens",
            ));
        }

        Ok(Self {
            pair_addr,
            lp_token,
            pool_assets: pair_info
                .asset_infos
                .into_iter()
                .map(astroport_v5_assetinfo_to_assetinfo)
                .collect(),
            pair_type: astroport_v5_pairtype_to_astroport_v2_pairtype(pair_info.pair_type),
            liquidity_manager,
        })
    }

    /// Returns the matching pool given a LP token.
    ///
    /// Arguments:
    /// - `lp_token`: Said LP token
    /// - `astroport_liquidity_manager`: The Astroport liquidity manager
    ///   address.
    pub fn get_pool_for_lp_token(
        deps: Deps,
        lp_token: &AssetInfo,
        astroport_liquidity_manager: Option<Addr>,
    ) -> Result<Self, CwDexError> {
        match lp_token {
            AssetInfo::Cw20(address) => {
                // To figure out if the CW20 is a LP token, we need to check which address
                // instantiated the CW20 and check if that address is an Astroport pair
                // contract.
                let contract_info = deps.querier.query_wasm_contract_info(address)?;
                let creator_addr = deps.api.addr_validate(&contract_info.creator)?;

                // Try to create an `AstroportPool` object with the creator address. This will
                // query the contract and assume that it is an Astroport pair
                // contract. If it succeeds, the pool object will be returned.
                //
                // NB: This does NOT validate that the pool is registered with the Astroport
                // factory, and that it is an "official" Astroport pool.
                let pool = AstroportPool::new(deps, creator_addr, astroport_liquidity_manager)?;

                Ok(pool)
            }
            AssetInfo::Native(native_denom) => {
                // To figure out if the native denom is a LP token, we need to check which
                // address created the native denom and check if that address is
                // an Astroport pair contract.
                let denom_authority_metadata = parse_address(native_denom)?;

                // Try to create an `AstroportPool` object with the creator address. This will
                // query the contract and assume that it is an Astroport pair
                // contract. If it succeeds, the pool object will be returned.
                //
                // NB: This does NOT validate that the pool is registered with the Astroport
                // factory, and that it is an "official" Astroport pool.
                let pool = AstroportPool::new(
                    deps,
                    Addr::unchecked(denom_authority_metadata),
                    astroport_liquidity_manager,
                )?;

                Ok(pool)
            }
        }
    }

    /// Returns the total supply of the associated LP token
    pub fn query_lp_token_supply(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        match &self.lp_token {
            AssetInfo::Native(denom) => Ok(querier.query_supply(denom)?.amount),
            AssetInfo::Cw20(token_addr) => query_supply(querier, token_addr),
        }
    }

    /// Queries the pair contract for the current pool state
    pub fn query_pool_info(&self, querier: &QuerierWrapper) -> StdResult<PoolResponse> {
        querier.query::<PoolResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.pair_addr.to_string(),
            msg: to_json_binary(&PairQueryMsg::Pool {})?,
        }))
    }
}

impl Pool for AstroportPool {
    fn provide_liquidity(
        &self,
        _deps: Deps,
        env: &Env,
        assets: AssetList,
        min_out: Uint128,
    ) -> Result<Response, CwDexError> {
        let (funds, cw20s) = separate_natives_and_cw20s(&assets);

        let contract_address = if let Some(liquidity_manager) = &self.liquidity_manager {
            liquidity_manager
        } else {
            // Ensure min_out is not set for concentrated liquidity pools, as they
            // do not support min_out
            match &self.pair_type {
                PairType::Custom(custom) if custom == "concentrated" => {
                    if min_out != Uint128::zero() {
                        return Err(CwDexError::MinOutNotSupported {});
                    }
                }
                _ => {}
            }

            &self.pair_addr
        };

        // Increase allowance on all Cw20s
        let allowance_msgs: Vec<CosmosMsg> = cw20s
            .into_iter()
            .map(|asset| {
                Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: asset.address,
                    msg: to_json_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: contract_address.to_string(),
                        amount: asset.amount,
                        expires: Some(Expiration::AtHeight(env.block.height + 1)),
                    })?,
                    funds: vec![],
                }))
            })
            .collect::<StdResult<Vec<_>>>()?;

        // Liquidity manager requires assets vec to contain all assets in the pool
        let mut assets_vec = assets.to_vec();
        for pool_asset_info in &self.pool_assets {
            if !assets_vec.iter().any(|x| &x.info == pool_asset_info) {
                assets_vec.push(Asset::new(pool_asset_info.clone(), Uint128::zero()));
            }
        }

        // Create the provide liquidity message
        let provide_liquidity_msg = match &self.liquidity_manager {
            Some(liquidity_manager) => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: liquidity_manager.to_string(),
                msg: to_json_binary(&liquidity_manager::ExecuteMsg::ProvideLiquidity {
                    pair_addr: self.pair_addr.to_string(),
                    min_lp_to_receive: Some(min_out),
                    pair_msg: astroport::pair::ExecuteMsg::ProvideLiquidity {
                        assets: assets_vec.into_elementwise(),
                        slippage_tolerance: Some(Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?),
                        auto_stake: Some(false),
                        receiver: None,
                    },
                })?,
                funds,
            }),
            None => CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: self.pair_addr.to_string(),
                msg: to_json_binary(&astroport_v5::pair::ExecuteMsg::ProvideLiquidity {
                    assets: assets_vec.iter().map(asset_to_astroport_v5_asset).collect(),
                    slippage_tolerance: Some(Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?),
                    auto_stake: Some(false),
                    receiver: None,
                    min_lp_to_receive: Some(min_out),
                })?,
                funds,
            }),
        };

        let event = Event::new("apollo/cw-dex/provide_liquidity")
            .add_attribute("pair_addr", &self.pair_addr)
            .add_attribute("assets", format!("{:?}", assets))
            .add_attribute("lp_token", format!("{:?}", &self.lp_token()));

        Ok(Response::new()
            .add_messages(allowance_msgs)
            .add_message(provide_liquidity_msg)
            .add_event(event))
    }

    fn withdraw_liquidity(
        &self,
        _deps: Deps,
        _env: &Env,
        asset: Asset,
        mut min_out: AssetList,
    ) -> Result<Response, CwDexError> {
        if let Some(liquidity_manager) = &self.liquidity_manager {
            // Liquidity manager requires min_out to contain all assets in the pool
            for asset in &self.pool_assets {
                if min_out.find(asset).is_none() {
                    // Add one unit as AssetList does not allow zero amounts (calls self.purge on
                    // add)
                    min_out.add(&Asset::new(asset.clone(), Uint128::one()))?;
                }
            }

            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: self.lp_token.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: liquidity_manager.to_string(),
                    amount: asset.amount,
                    msg: to_json_binary(&liquidity_manager::Cw20HookMsg::WithdrawLiquidity {
                        pair_msg: astroport::pair::Cw20HookMsg::WithdrawLiquidity {
                            // This field is currently not used...
                            assets: vec![],
                        },
                        min_assets_to_receive: min_out.to_vec().into_elementwise(),
                    })?,
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
        } else if let AssetInfoBase::Native(token_denom) = &asset.info {
            // Ensure min_out is not set for concentrated liquidity pools, as they
            // do not support min_out
            match &self.pair_type {
                PairType::Custom(custom) if custom == "concentrated" => {
                    if min_out.len() > 0 || min_out.into_iter().any(|x| x.amount > Uint128::zero())
                    {
                        return Err(CwDexError::MinOutNotSupported {});
                    }
                }
                _ => {}
            }

            let min_assets_to_receive = if min_out.len() > 0 {
                let mut min_assets: Vec<astroport_v5::asset::Asset> = vec![];
                // Astroport requires min_assets_to_receive to contain all assets in the pool
                for asset_info in &self.pool_assets {
                    match min_out.find(asset_info) {
                        Some(asset) => {
                            min_assets.push(asset_to_astroport_v5_asset(asset));
                        }
                        None => {
                            let asset = Asset::new(asset_info.clone(), Uint128::zero());
                            min_assets.push(asset_to_astroport_v5_asset(&asset));
                        }
                    }
                }
                Some(min_assets)
            } else {
                None
            };

            let withdraw_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: self.pair_addr.to_string(),
                msg: to_json_binary(&astroport_v5::pair::ExecuteMsg::WithdrawLiquidity {
                    assets: vec![],
                    min_assets_to_receive,
                })?,
                funds: coins(asset.amount.u128(), token_denom),
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
                        ask_asset_info: Some(ask_asset_info.to_owned().into()),
                    },
                    vec![offer_asset.clone().try_into()?],
                )
            }
            AssetInfo::Cw20(addr) => wasm_execute(
                addr.to_string(),
                &Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_json_binary(&PairCw20HookMsg::Swap {
                        belief_price,
                        max_spread: Some(Decimal::zero()),
                        to: Some(env.contract.address.to_string()),
                        ask_asset_info: Some(ask_asset_info.to_owned().into()),
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
        _env: &Env,
        assets: AssetList,
    ) -> Result<Asset, CwDexError> {
        if let Some(liquidity_manager) = &self.liquidity_manager {
            let amount: Uint128 = deps.querier.query_wasm_smart(
                liquidity_manager.to_string(),
                &liquidity_manager::QueryMsg::SimulateProvide {
                    pair_addr: self.pair_addr.to_string(),
                    pair_msg: astroport::pair::ExecuteMsg::ProvideLiquidity {
                        assets: assets.into(),
                        slippage_tolerance: Some(Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?),
                        auto_stake: Some(false),
                        receiver: None,
                    },
                },
            )?;

            let lp_token = Asset {
                info: self.lp_token.clone(),
                amount,
            };

            Ok(lp_token)
        } else {
            let amount: Uint128 = deps.querier.query_wasm_smart(
                self.pair_addr.to_string(),
                &astroport_v5::pair::QueryMsg::SimulateProvide {
                    assets: assets.iter().map(asset_to_astroport_v5_asset).collect(),
                    slippage_tolerance: Some(Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?),
                },
            )?;

            let lp_token = Asset {
                info: self.lp_token.clone(),
                amount,
            };

            Ok(lp_token)
        }
    }

    fn simulate_withdraw_liquidity(
        &self,
        deps: Deps,
        lp_token: &Asset,
    ) -> Result<AssetList, CwDexError> {
        let assets: Vec<AstroAsset> = if let Some(liquidity_manager) = &self.liquidity_manager {
            deps.querier.query_wasm_smart(
                liquidity_manager.to_string(),
                &liquidity_manager::QueryMsg::SimulateWithdraw {
                    pair_addr: self.pair_addr.to_string(),
                    lp_tokens: lp_token.amount,
                },
            )?
        } else {
            deps.querier.query_wasm_smart(
                self.pair_addr.to_string(),
                &astroport_v5::pair::QueryMsg::SimulateWithdraw {
                    lp_amount: lp_token.amount,
                },
            )?
        };

        Ok(assets.into())
    }

    fn simulate_swap(
        &self,
        deps: Deps,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
    ) -> StdResult<Uint128> {
        Ok(deps
            .querier
            .query::<SimulationResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.pair_addr.to_string(),
                msg: to_json_binary(&PairQueryMsg::Simulation {
                    offer_asset: offer_asset.into(),
                    ask_asset_info: Some(ask_asset_info.into()),
                })?,
            }))?
            .return_amount)
    }

    fn lp_token(&self) -> AssetInfo {
        self.lp_token.clone()
    }

    fn pool_assets(&self, _deps: Deps) -> StdResult<Vec<AssetInfo>> {
        Ok(self.pool_assets.clone())
    }
}

pub fn astroport_v5_assetinfo_to_assetinfo(asset: astroport_v5::asset::AssetInfo) -> AssetInfo {
    match asset {
        astroport_v5::asset::AssetInfo::NativeToken { denom } => AssetInfo::native(denom),
        astroport_v5::asset::AssetInfo::Token { contract_addr } => AssetInfo::cw20(contract_addr),
    }
}

pub fn asset_to_astroport_v5_asset(asset: &Asset) -> astroport_v5::asset::Asset {
    match &asset.info {
        AssetInfoBase::Native(denom) => astroport_v5::asset::Asset::native(denom, asset.amount),
        AssetInfo::Cw20(contract_addr) => {
            astroport_v5::asset::Asset::cw20(contract_addr.clone(), asset.amount)
        }
    }
}

pub fn astroport_v5_pairtype_to_astroport_v2_pairtype(
    pair_type: astroport_v5::factory::PairType,
) -> PairType {
    match pair_type {
        astroport_v5::factory::PairType::Xyk {} => PairType::Xyk {},
        astroport_v5::factory::PairType::Stable {} => PairType::Stable {},
        astroport_v5::factory::PairType::Custom(pair_type) => PairType::Custom(pair_type),
    }
}

fn parse_address(input_string: &str) -> Result<String, CwDexError> {
    let parts: Vec<&str> = input_string.split('/').collect();

    if parts.len() < 3 {
        return Err(CwDexError::AddressParsingErrors {
            token_denom: input_string.to_string(),
        });
    }

    Ok(parts[1].to_string())
}
