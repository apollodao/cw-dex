//! Contains exchange-specific implementations of the traits in the
//! `traits::pool` and `traits::staking` modules

#[cfg(feature = "astroport")]
#[cfg_attr(docsrs, doc(cfg(feature = "astroport")))]
pub mod astroport;

#[cfg(feature = "osmosis")]
#[cfg_attr(docsrs, doc(cfg(feature = "osmosis")))]
pub mod osmosis;

#[cfg(feature = "picasso")]
#[cfg_attr(docsrs, doc(cfg(feature = "picasso")))]
pub mod picasso;

pub mod pool;

pub use pool::*;
