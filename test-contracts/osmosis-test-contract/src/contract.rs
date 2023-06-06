use apollo_cw_asset::{Asset, AssetInfo, AssetList};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult,
    Uint128,
};
use cw_dex::osmosis::{OsmosisPool, OsmosisStaking, OsmosisSuperfluidStaking};
use cw_dex::traits::{ForceUnlock, Pool, Stake, Unlock};
// use cw2::set_contract_version;

use crate::error::ContractError;
use crate::state::{POOL, STAKING, SUPERFLUID};
use cw_dex_test_contract::msg::{ExecuteMsg, OsmosisTestContractInstantiateMsg, QueryMsg};

/*
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw-dex-test-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
*/

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: OsmosisTestContractInstantiateMsg,
) -> Result<Response, ContractError> {
    let pool = OsmosisPool::new(msg.pool_id, deps.as_ref())?;
    POOL.save(deps.storage, &pool)?;

    let lp_token_denom = pool.lp_token().to_string();

    if msg.lock_duration.is_none() && msg.superfluid_validator.is_none() {
        return Err(StdError::generic_err(
            "Must provide either lock_duration or superfluid_validator_addr",
        )
        .into());
    }

    if let Some(lock_duration) = msg.lock_duration {
        STAKING.save(
            deps.storage,
            &OsmosisStaking::new(lock_duration, Some(msg.lock_id), lp_token_denom.clone())?,
        )?;
    }

    if let Some(validator) = msg.superfluid_validator {
        SUPERFLUID.save(
            deps.storage,
            &OsmosisSuperfluidStaking::new(validator, Some(msg.lock_id), lp_token_denom)?,
        )?;
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ProvideLiquidity { assets, min_out } => {
            execute_provide_liquidity(deps, env, info, assets, min_out)
        }
        ExecuteMsg::WithdrawLiquidity { amount, min_out } => {
            execute_withdraw_liquidity(deps, env, info, amount, min_out)
        }
        ExecuteMsg::Stake { amount } => execute_stake(deps, env, info, amount),
        ExecuteMsg::Unlock { amount } => execute_unlock(deps, env, info, amount),
        ExecuteMsg::ForceUnlock { amount, lockup_id } => {
            execute_force_unlock(deps, env, info, amount, lockup_id)
        }
        ExecuteMsg::WithdrawUnlocked { amount } => {
            execute_withdraw_unlocked(deps, env, info, amount)
        }
        ExecuteMsg::Swap {
            offer,
            ask,
            min_out,
        } => execute_swap(deps, env, offer, ask, min_out),
        ExecuteMsg::SuperfluidStake { amount } => execute_superfluid_stake(deps, env, info, amount),
        ExecuteMsg::SuperfluidUnlock { amount } => {
            execute_superfluid_unlock(deps, env, info, amount)
        }
    }
}

pub fn execute_provide_liquidity(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    assets: AssetList,
    min_out: Uint128,
) -> Result<Response, ContractError> {
    let pool = POOL.load(deps.storage)?;

    Ok(pool.provide_liquidity(deps.as_ref(), &env, assets, min_out)?)
}

pub fn execute_withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
    min_out: AssetList,
) -> Result<Response, ContractError> {
    let pool = POOL.load(deps.storage)?;
    let lp_token = Asset {
        info: pool.lp_token(),
        amount,
    };

    Ok(pool.withdraw_liquidity(deps.as_ref(), &env, lp_token, min_out)?)
}

pub fn execute_stake(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;

    Ok(staking.stake(deps.as_ref(), &env, amount)?)
}

pub fn execute_unlock(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;

    Ok(staking.unlock(deps.as_ref(), &env, amount)?)
}

pub fn execute_superfluid_stake(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = SUPERFLUID
        .may_load(deps.storage)?
        .ok_or(StdError::generic_err("Superfluid staking not set"))?;

    Ok(staking.stake(deps.as_ref(), &env, amount)?)
}

pub fn execute_superfluid_unlock(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = SUPERFLUID
        .may_load(deps.storage)?
        .ok_or(StdError::generic_err("Superfluid staking not set"))?;

    Ok(staking.unlock(deps.as_ref(), &env, amount)?)
}

pub fn execute_withdraw_unlocked(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;

    Ok(staking.withdraw_unlocked(deps.as_ref(), &env, amount)?)
}

pub fn execute_force_unlock(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
    lockup_id: Option<u64>,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;

    Ok(staking.force_unlock(deps.as_ref(), &env, lockup_id, amount)?)
}

pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    offer: Asset,
    ask: AssetInfo,
    min_out: Uint128,
) -> Result<Response, ContractError> {
    let pool = POOL.load(deps.storage)?;

    Ok(pool.swap(deps.as_ref(), &env, offer, ask, min_out)?)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let pool = POOL.load(deps.storage)?;
    match msg {
        QueryMsg::PoolLiquidity {} => to_binary(&pool.get_pool_liquidity(deps)?),
        QueryMsg::SimulateProvideLiquidity { assets } => {
            to_binary(&pool.simulate_provide_liquidity(deps, &env, assets)?.amount)
        }
        QueryMsg::SimulateSwap { offer, ask, sender } => {
            query_simulate_swap(deps, offer, ask, sender)
        }
    }
}

pub fn query_simulate_swap(
    deps: Deps,
    offer: Asset,
    ask: AssetInfo,
    to: Option<String>,
) -> StdResult<Binary> {
    let pool = POOL.load(deps.storage)?;
    to_binary(&pool.simulate_swap(deps, offer, ask, to)?)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg(test)]
mod tests {}
