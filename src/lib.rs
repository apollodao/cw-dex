#![warn(rust_2021_compatibility, future_incompatible, nonstandard_style)]
#![forbid(unsafe_code)]
#![deny(bare_trait_objects, unused_doc_comments, unused_import_braces)]
#![warn(missing_docs)]

//! # cw-dex
//! CosmWasm abstractions for decentralized exchanges. This crate defines
//! a set of traits to abstract out the common behavior of various
//! decentralized exchanges so that the same contract code can be used to
//! interact with any of them.
//! 
//! The currently supported decentralized exchanges are:
//! - [Osmosis](cw_dex::implementations::osmosis)
//! - [Astroport](cw_dex::implementations::astroport)
//! - [Junoswap](cw_dex::implementations::Junoswap) 

pub mod error;
pub mod implementations;
pub mod traits;
mod utils;

pub use error::*;
pub use implementations::*;

// #[cfg(test)]
// pub mod tests;
