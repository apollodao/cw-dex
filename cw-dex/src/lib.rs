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
//! - [Osmosis]
//!    - Via crate `cw-dex-osmosis`
//! - [Astroport]
//!    - Via crate `cw-dex-astroport`

pub mod error;
pub mod traits;

#[deprecated(
    since = "0.5.2",
    note = "Please use separate implementation crates such as `cw-dex-astroport`, and `cw-dex-osmosis` instead"
)]
pub mod implementations;

pub use error::*;
#[allow(deprecated)]
pub use implementations::*;
