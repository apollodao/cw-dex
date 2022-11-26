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
//! - [Osmosis](crate::implementations::osmosis)
//! - [Astroport](crate::implementations::astroport)
//! - [Junoswap](crate::implementations::junoswap)

pub mod error;
pub mod implementations;
pub mod traits;

pub use error::*;
pub use implementations::*;

// #[cfg(test)]
// pub mod tests;
