use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Uint128, Uint64};

pub type PoolId = Uint128;

#[cw_serde]
#[derive(QueryResponses)]
pub enum ExecuteMsg {
    /// Like Osmosis MsgJoinPool
    #[returns(AddLiquidityResponse)]
    AddLiquidity {
        pool_id: PoolId,
        assets: Vec<Coin>,
        min_mint_amount: Uint128,
        keep_alive: bool,
    },
    /// Like Osmosis MsgExitPool
    #[returns(RemoveLiquidityResponse)]
    RemoveLiquidity {
        pool_id: PoolId,
        lp_amount: Uint128,
        min_receive: Vec<Coin>,
    },
    /// Like Osmosis MsgSwapExactAmountOut
    #[returns(BuyResponse)]
    Buy {
        pool_id: PoolId,
        in_asset_id: String,
        out_asset: Coin,
        keep_alive: bool,
    },
    /// Like Osmosis MsgSwapExactAmountIn
    #[returns(SwapResponse)]
    Swap {
        pool_id: PoolId,
        in_asset: Coin,
        min_receive: Coin,
        keep_alive: bool,
    },
}

#[cw_serde]
pub struct AddLiquidityResponse {
    pub lp_amount: Uint128,
}

#[cw_serde]
pub struct RemoveLiquidityResponse {
    pub assets: Vec<Coin>,
}

#[cw_serde]
pub struct BuyResponse {
    pub value: Coin,
    pub fee: Coin,
}

#[cw_serde]
pub struct SwapResponse {
    pub value: Coin,
    pub fee: Coin,
}

#[cw_serde]
pub struct AssetsResponse {
    pub assets: Vec<(String, (Uint64, Uint64))>,
}

#[cw_serde]
pub struct LpTokenResponse {
    pub lp_token: String,
}

#[cw_serde]
pub struct RedeemableAssetsForLpTokensResponse {
    pub assets: Vec<Coin>,
}

#[cw_serde]
pub struct SimulateAddLiquidityResponse {
    pub amount: Uint128,
}

#[cw_serde]
pub struct SimulateRemoveLiquidityResponse {
    pub amounts: Vec<Coin>,
}

#[cw_serde]
pub struct SpotPriceResponse {
    pub value: Coin,
    pub fee: Coin,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// total supply of any assets can be asked from Bank as we share all tokens
    /// here
    #[returns(AssetsResponse)]
    Assets { pool_id: PoolId },
    #[returns(SpotPriceResponse)]
    SpotPrice {
        pool_id: PoolId,
        base_asset: Coin,
        quote_asset_id: String,
        calculate_with_fees: bool,
    },
    #[returns(LpTokenResponse)]
    LpToken { pool_id: PoolId },
    #[returns(RedeemableAssetsForLpTokensResponse)]
    RedeemableAssetsForLpTokens { pool_id: PoolId, lp_amount: Uint128 },
    #[returns(SimulateAddLiquidityResponse)]
    SimulateAddLiquidity { pool_id: PoolId, amounts: Vec<Coin> },
    #[returns(SimulateRemoveLiquidityResponse)]
    SimulateRemoveLiquidity {
        pool_id: PoolId,
        lp_amount: Uint128,
        min_amount: Vec<Coin>,
    },
}
