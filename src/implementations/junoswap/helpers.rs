use apollo_utils::assets::separate_natives_and_cw20s;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, Env, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use cw20_0_10_3::Denom;
use cw_asset::{Asset, AssetInfo, AssetList};
use wasmswap::msg::InfoResponse;

use crate::CwDexError;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JunoAssetInfo(pub(crate) Denom);

impl TryFrom<AssetInfo> for JunoAssetInfo {
    type Error = StdError;
    fn try_from(info: AssetInfo) -> StdResult<Self> {
        match info {
            AssetInfo::Native(denom) => Ok(JunoAssetInfo(Denom::Native(denom))),
            AssetInfo::Cw20(addr) => Ok(JunoAssetInfo(Denom::Cw20(addr))),
        }
    }
}

impl From<JunoAssetInfo> for AssetInfo {
    fn from(info: JunoAssetInfo) -> Self {
        match info.0 {
            Denom::Native(denom) => AssetInfo::Native(denom),
            Denom::Cw20(addr) => AssetInfo::Cw20(addr),
        }
    }
}

impl From<Denom> for JunoAssetInfo {
    fn from(denom: Denom) -> Self {
        JunoAssetInfo(denom)
    }
}

impl PartialEq<AssetInfo> for JunoAssetInfo {
    fn eq(&self, other: &AssetInfo) -> bool {
        match self {
            JunoAssetInfo(Denom::Native(denom)) => match other {
                AssetInfo::Native(other_denom) => denom == other_denom,
                _ => false,
            },
            JunoAssetInfo(Denom::Cw20(addr)) => match other {
                AssetInfo::Cw20(other_addr) => addr == other_addr,
                _ => false,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct JunoAsset {
    pub(crate) info: JunoAssetInfo,
    pub(crate) amount: Uint128,
}

impl TryFrom<&Asset> for JunoAsset {
    type Error = StdError;
    fn try_from(asset: &Asset) -> StdResult<Self> {
        Ok(Self {
            info: asset.info.clone().try_into()?,
            amount: asset.amount,
        })
    }
}

impl From<JunoAsset> for Asset {
    fn from(asset: JunoAsset) -> Self {
        Self {
            info: asset.info.into(),
            amount: asset.amount,
        }
    }
}
pub struct JunoAssetList(pub(crate) Vec<JunoAsset>);

impl TryFrom<AssetList> for JunoAssetList {
    type Error = StdError;
    fn try_from(list: AssetList) -> StdResult<Self> {
        Ok(Self(
            list.into_iter()
                .map(|a| a.try_into())
                .collect::<StdResult<Vec<_>>>()?,
        ))
    }
}

impl From<JunoAssetList> for AssetList {
    fn from(list: JunoAssetList) -> Self {
        list.0
            .into_iter()
            .map(|a| Asset {
                info: match a.info {
                    JunoAssetInfo(Denom::Native(denom)) => AssetInfo::Native(denom),
                    JunoAssetInfo(Denom::Cw20(addr)) => AssetInfo::Cw20(addr),
                },
                amount: a.amount,
            })
            .collect::<Vec<_>>()
            .into()
    }
}

impl<'a> IntoIterator for &'a JunoAssetList {
    type Item = &'a JunoAsset;
    type IntoIter = std::slice::Iter<'a, JunoAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl JunoAssetList {
    pub(crate) fn find(&self, token: JunoAssetInfo) -> StdResult<&JunoAsset> {
        self.into_iter()
            .find(|a| a.info == token)
            .ok_or_else(|| StdError::generic_err("Token not found in JunoAssetList instance"))
    }
}

/// Prepare the `funds` vec to send native tokens to a contract and construct
/// the messages to increase allowance for cw20 tokens.
///
/// ### Returns
/// `(funds, messages)` tuple where,
/// - `funds` is a `Vec<Coin> of the native tokens present in the `assets` list
///   that were also exist in the `info.funds` list.
/// - `increase_allowances` is a `Vec<CosmosMsg>` with the messages to increase
///   allowance for the CW20 tokens in the `assets` list.
pub(crate) fn prepare_funds_and_increase_allowances(
    env: &Env,
    assets: &AssetList,
    spender: &Addr,
) -> Result<(Vec<Coin>, Vec<CosmosMsg>), CwDexError> {
    let (funds, cw20s) = separate_natives_and_cw20s(assets);

    // Build increase allowance messages for cw20 tokens
    let increase_allowances = cw20s
        .into_iter()
        .map(|cw20| {
            Ok(WasmMsg::Execute {
                contract_addr: cw20.address,
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: spender.to_string(),
                    amount: cw20.amount,
                    expires: Some(cw20::Expiration::AtHeight(env.block.height + 1)),
                })?,
                funds: vec![],
            }
            .into())
        })
        .collect::<StdResult<Vec<CosmosMsg>>>()?;

    Ok((funds, increase_allowances))
}

// ------------------ Junoswap math ----------------------

/// Returns the amount lp tokens minted for a given amount of token1 on Junoswap
///
/// Copied from WasmSwap source code:
/// https://github.com/Wasmswap/wasmswap-contracts/blob/8781ab0da9de4a3bfcb071ffb59b6547e7215118/src/contract.rs#L206-L220
pub(crate) fn juno_get_lp_token_amount_to_mint(
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, CwDexError> {
    if liquidity_supply == Uint128::zero() {
        Ok(token1_amount)
    } else {
        Ok(token1_amount
            .checked_mul(liquidity_supply)?
            .checked_div(token1_reserve)?)
    }
}

/// Returns the amount of token2 required to match the given amount of token1
/// when providing liquidity on Junoswap
///
/// Copied from WasmSwap source code:
/// https://github.com/Wasmswap/wasmswap-contracts/blob/8781ab0da9de4a3bfcb071ffb59b6547e7215118/src/contract.rs#L222-L240
pub(crate) fn juno_get_token2_amount_required(
    max_token: Uint128,
    token1_amount: Uint128,
    liquidity_supply: Uint128,
    token2_reserve: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, CwDexError> {
    if liquidity_supply == Uint128::zero() {
        Ok(max_token)
    } else {
        Ok(token1_amount
            .checked_mul(token2_reserve)?
            .checked_div(token1_reserve)?
            .checked_add(Uint128::new(1))?)
    }
}

/// This is the reverse calculation of `juno_get_token2_amount_required`.
/// This code does not exist in Junoswap codebase, but we need it to calculate
/// how many assets to send when providing liquidity. See Junoswap
/// `provide_liquidity` implementation for why.
pub(crate) fn juno_get_token1_amount_required(
    token2_amount: Uint128,
    token2_reserve: Uint128,
    token1_reserve: Uint128,
) -> Result<Uint128, CwDexError> {
    Ok(token2_amount
        .checked_sub(Uint128::one())?
        .checked_mul(token1_reserve)?
        .checked_div(token2_reserve)?)
}

pub(crate) struct JunoProvideLiquidityInfo {
    pub token1_to_use: Asset,
    pub token2_to_use: Asset,
    pub lp_token_expected_amount: Uint128,
}

/// ### Returns
/// The amount of token1 and token2 that should be sent to Junoswap to provide
/// liquidity and the expected amount of lp tokens to be minted.
pub(crate) fn juno_simulate_provide_liquidity(
    assets: &JunoAssetList,
    pool_info: InfoResponse,
) -> Result<JunoProvideLiquidityInfo, CwDexError> {
    let token1 = assets.find(pool_info.token1_denom.into())?;
    let token2 = assets.find(pool_info.token2_denom.into())?;

    // Junoswap requires us to specify how many token1 we want to use and
    // calculates itself how many token2 are needed to use the specified
    // amount of token1. Therefore we send (or approve spend) at least this
    // amount of token2 that Junoswap calculates internally. However,
    // we don't want to send extra, nor approve spend on extra, and we want
    // to use as much of both token1 and token2 as possible, so we must
    // calculate exactly how much of each to send.
    // Therefore, we must first check the ratio of assets in the pool and
    // compare with the ratio of assets that are sent to this function to
    // determine which of the assets to use all of and which to not use all of.
    let pool_ratio =
        Decimal::checked_from_ratio(pool_info.token1_reserve, pool_info.token2_reserve)
            .unwrap_or_default();
    let asset_ratio = Decimal::checked_from_ratio(token1.amount, token2.amount).unwrap_or_default();

    let token1_to_use;
    let token2_to_use;

    if pool_ratio < asset_ratio {
        // We have a higher ratio of token 1 than the pool, so if we try to use
        // all of our token1 we will get an error because we don't have enough
        // token2. So we must calculate how much of token1 we should use
        // assuming we want to use all of token2.
        token2_to_use = token2.amount;
        token1_to_use = juno_get_token1_amount_required(
            token2_to_use,
            pool_info.token1_reserve,
            pool_info.token2_reserve,
        )?;
    } else {
        // We have a higher ratio of token 2 than token1, so calculate how much
        // token2 to use (and approve spend for, since we don't want to approve
        // spend on any extra).
        token1_to_use = token1.amount;
        token2_to_use = juno_get_token2_amount_required(
            token2.amount,
            token1.amount,
            pool_info.lp_token_supply,
            pool_info.token2_reserve,
            pool_info.token1_reserve,
        )?;
    }

    let expected_lps = juno_get_lp_token_amount_to_mint(
        token1_to_use,
        pool_info.lp_token_supply,
        pool_info.token1_reserve,
    )?;

    Ok(JunoProvideLiquidityInfo {
        token1_to_use: Asset {
            amount: token1_to_use,
            info: token1.info.clone().into(),
        },
        token2_to_use: Asset {
            amount: token2_to_use,
            info: token2.info.clone().into(),
        },
        lp_token_expected_amount: expected_lps,
    })
}
