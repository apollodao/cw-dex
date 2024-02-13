use cw_dex_osmosis::{OsmosisPool, OsmosisStaking, OsmosisSuperfluidStaking};
use cw_storage_plus::Item;

pub const POOL: Item<OsmosisPool> = Item::new("pool");
pub const STAKING: Item<OsmosisStaking> = Item::new("staking");
pub const SUPERFLUID: Item<OsmosisSuperfluidStaking> = Item::new("superfluid");
