use std::num::TryFromIntError;

use cosmwasm_std::StdError;
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

    /// Invalid Reply ID Error
    #[error("invalid output asset")]
    InvalidOutAsset {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.31/thiserror/ for details.
    #[error("invalid input asset: {a}")]
    InvalidInAsset {
        a: Asset,
    },
}

impl Into<StdError> for CwDexError {
    fn into(self) -> StdError {
        StdError::GenericErr {
            msg: self.to_string(),
        }
    }
}
