#![warn(rust_2021_compatibility, future_incompatible, nonstandard_style)]
#![forbid(unsafe_code)]
#![deny(bare_trait_objects, unused_doc_comments, unused_import_braces)]
#![warn(missing_docs)]

//! This is `cw-dex` (wow!)

pub mod error;
pub mod implementations;
pub mod traits;
mod utils;

pub use error::*;
pub use implementations::*;

// #[cfg(test)]
// pub mod tests;
