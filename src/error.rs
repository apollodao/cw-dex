use cosmwasm_std::StdError;
use thiserror::Error;

/// ## Description
/// This enum describes router-test contract errors!
#[derive(Error, Debug, PartialEq)]

pub enum CwDexError {
    #[error("{0}")]
    Std(#[from] StdError),

    /// Invalid Reply ID Error
    #[error("invalid reply id")]
    InvalidReplyId {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.31/thiserror/ for details.
}
