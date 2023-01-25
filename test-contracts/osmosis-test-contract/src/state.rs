use cw_dex::osmosis::{OsmosisPool, OsmosisStaking};
use cw_storage_plus::Item;

pub const POOL: Item<OsmosisPool> = Item::new("pool");
pub const STAKING: Item<OsmosisStaking> = Item::new("staking");
