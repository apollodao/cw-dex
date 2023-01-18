use cw_dex::astroport::{AstroportPool, AstroportStaking};
use cw_storage_plus::Item;

pub const POOL: Item<AstroportPool> = Item::new("pool");
pub const STAKING: Item<AstroportStaking> = Item::new("staking");
