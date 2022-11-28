//! Contains exchange-specific implementations of the traits in the
//! `traits::pool` and `traits::staking` modules

#[cfg(feature = "astroport")]
#[cfg_attr(docsrs, doc(cfg(feature = "astroport")))]
pub mod astroport;

#[cfg(feature = "junoswap")]
#[cfg_attr(docsrs, doc(cfg(feature = "junoswap")))]
pub mod junoswap;

#[cfg(feature = "osmosis")]
#[cfg_attr(docsrs, doc(cfg(feature = "osmosis")))]
pub mod osmosis;

pub mod pool;

pub use pool::*;
