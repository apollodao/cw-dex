use std::num::TryFromIntError;

use cosmwasm_std::{Decimal, DivideByZeroError, OverflowError, StdError, Uint128};
use cw_asset::Asset;
use thiserror::Error;

/// ## Description
/// This enum describes router-test contract errors!
#[derive(Error, Debug, PartialEq)]

pub enum CwDexError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    TryFromIntError(#[from] TryFromIntError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    DivideByZero(#[from] DivideByZeroError),

    /// Invalid Reply ID Error
    #[error("invalid output asset")]
    InvalidOutAsset {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.31/thiserror/ for details.
    #[error("invalid input asset: {a}")]
    InvalidInAsset {
        a: Asset,
    },

    #[error("Overflow when converting to from BigInt to Uint128")]
    BigIntOverflow {},

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Insufficient amount of liquidity")]
    LiquidityAmountTooSmall {},

    #[error("It is not possible to provide liquidity with one token for an empty pool")]
    InvalidProvideLPsWithSingleToken {},

    #[error("Slippage control failed. Wanted minimum {wanted} but got {got}")]
    SlippageControlMinOutFailed {
        wanted: Uint128,
        got: Uint128,
    },

    #[error("Slippage control failed because price moved too much.")]
    SlippageControlPriceFailed {
        old_price: Decimal,
        new_price: Decimal,
    },

    #[error("Asset is not an LP token")]
    NotLpToken {},

    #[error("Expected no unbonding period")]
    UnstakingDurationNotSupported {},
}

impl From<CwDexError> for StdError {
    fn from(x: CwDexError) -> Self {
        Self::GenericErr {
            msg: String::from("CwDexError: ") + &x.to_string(),
        }
    }
}
