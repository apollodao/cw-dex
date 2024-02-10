use crate::error::ContractError;
use crate::state::{POOL, STAKING};
use apollo_cw_asset::{Asset, AssetInfo, AssetList};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult,
    Uint128,
};
use cw_dex::astroport::{AstroportPool, AstroportStaking};
use cw_dex::traits::{Pool, Rewards, Stake, Unstake};
use cw_dex_test_contract::msg::{
    AstroportContractInstantiateMsg as InstantiateMsg, AstroportExecuteMsg as ExecuteMsg, QueryMsg,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let pair_addr = deps.api.addr_validate(&msg.pair_addr)?;
    let liquidity_manager = deps.api.addr_validate(&msg.liquidity_manager_addr)?;
    let pool = AstroportPool::new(deps.as_ref(), pair_addr, liquidity_manager)?;
    POOL.save(deps.storage, &pool)?;

    STAKING.save(
        deps.storage,
        &AstroportStaking {
            lp_token_addr: Addr::unchecked(msg.lp_token_addr),

            incentives: Addr::unchecked(msg.incentives_addr),
        },
    )?;

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
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::ClaimRewards {} => execute_claim_rewards(deps, env, info),
        ExecuteMsg::Swap {
            offer,
            ask,
            min_out,
        } => execute_swap(deps, env, offer, ask, min_out),
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

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;
    Ok(staking.unstake(deps.as_ref(), &env, amount)?)
}

pub fn execute_claim_rewards(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> Result<Response, ContractError> {
    let staking = STAKING.load(deps.storage)?;
    Ok(staking.claim_rewards(deps.as_ref(), &env)?)
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
        QueryMsg::PoolLiquidity {} => to_json_binary(&pool.get_pool_liquidity(deps)?),
        QueryMsg::SimulateProvideLiquidity { assets } => {
            to_json_binary(&pool.simulate_provide_liquidity(deps, &env, assets)?.amount)
        }
        QueryMsg::SimulateWithdrawLiquidty { amount } => to_json_binary(
            &pool.simulate_withdraw_liquidity(deps, &Asset::new(pool.lp_token(), amount))?,
        ),
        QueryMsg::SimulateSwap { offer, ask } => query_simulate_swap(deps, offer, ask),
        QueryMsg::GetPoolForLpToken { lp_token } => to_json_binary(
            &cw_dex::Pool::get_pool_for_lp_token(deps, &lp_token, Some(pool.liquidity_manager))?,
        ),
    }
}

pub fn query_simulate_swap(deps: Deps, offer: Asset, ask: AssetInfo) -> StdResult<Binary> {
    let pool = POOL.load(deps.storage)?;
    to_json_binary(&pool.simulate_swap(deps, offer, ask)?)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg(test)]
mod tests {}
