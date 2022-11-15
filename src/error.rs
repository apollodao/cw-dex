//! This crate contains the enum CwDexError with variants for contract errors,
//! and related functions

use std::num::TryFromIntError;

use cosmwasm_std::{DivideByZeroError, OverflowError, StdError};
use cw_asset::Asset;
use thiserror::Error;

/// ## Description
/// This enum describes router-test contract errors!
#[derive(Error, Debug, PartialEq)]

pub enum CwDexError {
    /// Converts from `cosmwasm_std::StdError`
    #[error("{0}")]
    Std(#[from] StdError),

    /// Converts from `std::num::TryFromIntError`
    #[error("{0}")]
    TryFromIntError(#[from] TryFromIntError),

    /// Converts from `cosmwasm_std::OverflowError`
    #[error("{0}")]
    Overflow(#[from] OverflowError),

    /// Converts from `cosmwasm_std::DivideByZeroError`
    #[error("{0}")]
    DivideByZero(#[from] DivideByZeroError),

    /// Invalid Reply ID Error
    #[error("Invalid output asset")]
    InvalidOutAsset {},

    /// Invalid input asset
    #[error("Invalid input asset: {a}")]
    InvalidInAsset {
        /// The asset in question
        a: Asset,
    },

    /// Overflow when converting to from BigInt to Uint128
    #[error("Overflow when converting to from BigInt to Uint128")]
    BigIntOverflow {},

    /// Zero funds transfer
    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    /// Insufficient amount of liquidity
    #[error("Insufficient amount of liquidity")]
    LiquidityAmountTooSmall {},

    /// Results from single-sided entry into empty pool
    #[error("It is not possible to provide liquidity with one token for an empty pool")]
    InvalidProvideLPsWithSingleToken {},

    /// Asset is not an LP token
    #[error("Asset is not an LP token")]
    NotLpToken {},

    /// When unstaking/unbonding is expected to be instant
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
