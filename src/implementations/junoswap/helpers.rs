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
            .ok_or(StdError::generic_err(
                "Token not found in JunoAssetList instance",
            ))
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
/// 
/// ### Requirements
/// - pool_info.token2_reserve must not be zero since it is used as denominator
/// - token2.amount must not be zero since it is used as denominator
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{ Addr, StdError, Uint128, Decimal };
    use cw20_0_10_3::Denom;
    use test_case::test_case;
    use wasmswap::msg::InfoResponse;
    
    
    // Edge borders testing
    #[test_case(100,0,100,1 => matches Err(_); "when reserve_a is zero should err")]
    #[test_case(100,1,100,0 => Ok((0, 100, 0)); "when reserve_b is zero initial state or unbalance should OK")]
    #[test_case(100,1,0,1 => Ok((100, 101, 200)); "when amount_b is zero initial state or unbalance should OK")]
    #[test_case(0,1,0,1 => Ok( (0,1,0) ); "when amount_a and amount_b zero should work")] // TODO: Is this OK? The return value for token2_to_use is 1
    #[test_case(30,1,1,1 => with |i: Result<(u128,u128,u128),StdError> | assert!(i.unwrap().1 == 1u128); "when asset_ratio gt pool_ratio with amount_a gt amount_b")] 
    #[test_case(1,2,1,2 => Ok( (1,2,2) ); "when pool_ratio greater than asset_ratio")]
    fn juno_simulate_provide_liquidity_test(
        amount_a: u128,
        reserve_a: u128,
        amount_b: u128,
        reserve_b: u128
    ) -> Result<(u128, u128, u128), StdError> {
        let usdc = JunoAsset {
            info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("usdc"))),
            amount: Uint128::from(amount_a),
        };
        let dai = JunoAsset {
            info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("dai"))),
            amount: Uint128::from(amount_b),
        };
        let assets: JunoAssetList = JunoAssetList(vec![usdc.to_owned(), dai.to_owned()]);
        let pool_info: InfoResponse = InfoResponse {
            token1_reserve: Uint128::from(reserve_a),
            token1_denom: usdc.info.0,
            token2_reserve: Uint128::from(reserve_b),
            token2_denom: dai.info.0,
            lp_token_supply: Uint128::new(reserve_a) + Uint128::new(reserve_b),
            lp_token_address: "lp_token_address".to_string(),
            owner: Some("owner".to_string()),
            lp_fee_percent: Decimal::new(Uint128::from(0u128)),
            protocol_fee_percent: Decimal::new(Uint128::from(0u128)),
            protocol_fee_recipient: "protocol_fee_recipient_addr".to_string(),
        };
    
        let result: JunoProvideLiquidityInfo = juno_simulate_provide_liquidity(&assets, pool_info)?;

        Ok((
            result.token1_to_use.amount.u128(),
            result.token2_to_use.amount.u128(),
            result.lp_token_expected_amount.u128(),
        ))
    }
    
    #[test]
    #[should_panic]
    fn juno_simulate_provide_liquidity_test_tokens_not_found() {
        let usdc = JunoAsset {
            info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("usdc"))),
            amount: Uint128::from(1u128),
        };
        let dai = JunoAsset {
            info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("dai"))),
            amount: Uint128::from(1u128),
        };
        let rare = JunoAsset {
            info: JunoAssetInfo(Denom::Cw20(Addr::unchecked("rare"))),
            amount: Uint128::from(1u128),
        };
        let assets: JunoAssetList = JunoAssetList(vec![rare, dai.to_owned()]);
        let pool_info: InfoResponse = InfoResponse {
            token1_reserve: Uint128::from(1u128),
            token1_denom: usdc.info.0,
            token2_reserve: Uint128::from(1u128),
            token2_denom: dai.info.0,
            lp_token_supply: usdc.amount + dai.amount,
            lp_token_address: "lp_token_address".to_string(),
            owner: Some("owner".to_string()),
            lp_fee_percent: Decimal::new(Uint128::from(0u128)),
            protocol_fee_percent: Decimal::new(Uint128::from(0u128)),
            protocol_fee_recipient: "protocol_fee_recipient_addr".to_string(),
        };
    
        juno_simulate_provide_liquidity(&assets, pool_info).unwrap();
    }
}
