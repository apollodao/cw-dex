//! Module containing Pool and Staking implementations for Osmosis
/// Some functions are needed by Astroport to calculate how many LP shares a
/// user should get when providing liquidity but is not publicly exposed in the
/// package. Original code from:
/// <https://github.com/astroport-fi/astroport-core/blob/f1caf2e4cba74d60ff0e8ae3abba9d9e1f88c06e>
pub mod helpers;

pub mod msg;
mod pool;
mod staking;

pub use pool::AstroportPool;
pub use staking::AstroportStaking;
