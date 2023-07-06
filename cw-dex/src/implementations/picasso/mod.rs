//! Module containing Pool implementation for Picasso

// about picasso dex:
// 1. we are  no_std, so cannot directly share our crates, so copy pasted
// messages 2. we are precompile contract, not module or host extension - so we
// have constant contract address 3. all tokens/assets are just numbers which
// are ToString denom

mod msg;
mod pool;
pub use pool::*;
