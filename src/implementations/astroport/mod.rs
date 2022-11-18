/// These functions are needed to calculate how many LP shares a user should get when providing liquidity but is
/// not publicly exposed in the package. Copied from Astroport's implementation here:
/// https://github.com/astroport-fi/astroport-core/blob/f1caf2e4cba74d60ff0e8ae3abba9d9e1f88c06e/contracts/pair_stable
mod helpers;
mod pool;
mod staking;

pub use pool::AstroportPool;
pub use staking::AstroportStaking;
