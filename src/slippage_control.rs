use std::fmt::Display;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cosmwasm_std::{Decimal, StdError, StdResult};
use cw_asset::AssetInfo;

use crate::error::CwDexError;

#[cw_serde]
pub struct Price {
    pub base_asset: AssetInfo,
    pub quote_asset: AssetInfo,
    pub price: Decimal,
}

impl Display for Price {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} per {}", self.price, self.quote_asset, self.base_asset)
    }
}

impl Price {
    pub fn get_price(&self, base_asset: &AssetInfo, quote_asset: &AssetInfo) -> StdResult<Decimal> {
        if base_asset == &self.base_asset && quote_asset == &self.quote_asset {
            Ok(self.price)
        } else if base_asset == &self.quote_asset && quote_asset == &self.base_asset {
            Ok(Decimal::one() / self.price)
        } else {
            Err(StdError::generic_err("Price not for requested assets"))
        }
    }
}

/// Options for slippage control when providing liquidity (TODO: and swapping?)
#[cw_serde]
pub enum SlippageControl {
    /// Require a minimum amount of LP tokens to be returned
    MinOut(Uint128),
    /// The user supplies a belief about the current price and the transaction
    /// reverts if the resulting price is more than `slippage_tolerance` percent
    /// different from `belief_price`.
    BeliefPrice {
        belief_price: Price,
        slippage_tolerance: Decimal,
    },
    /// Require that the price in the pool does not move more than
    /// `max_price_impact` from the current price before this transaction.
    MaxPriceImpact {
        max_price_impact: Decimal,
    },
}

impl SlippageControl {
    pub fn assert(
        &self,
        old_price: Price,
        new_price: Price,
        shares_returned: Uint128,
    ) -> Result<(), CwDexError> {
        match self {
            SlippageControl::MinOut(min_out) => {
                if &shares_returned < min_out {
                    return Err(CwDexError::SlippageControlMinOutFailed {
                        wanted: *min_out,
                        got: shares_returned,
                    });
                }
            }
            SlippageControl::BeliefPrice {
                belief_price,
                slippage_tolerance,
            } => {
                let new_price_in_correct_quote =
                    new_price.get_price(&belief_price.base_asset, &belief_price.quote_asset)?;

                let max_price = belief_price.price * (Decimal::one() + *slippage_tolerance);
                let min_price = belief_price.price * (Decimal::one() - *slippage_tolerance);
                if new_price_in_correct_quote > max_price || new_price_in_correct_quote < min_price
                {
                    return Err(CwDexError::SlippageControlPriceFailed {
                        old_price,
                        new_price,
                    });
                }
            }
            SlippageControl::MaxPriceImpact {
                max_price_impact,
            } => {
                let max_price = old_price.price * (Decimal::one() + *max_price_impact);
                let min_price = old_price.price * (Decimal::one() - *max_price_impact);
                if new_price.price > max_price || new_price.price < min_price {
                    return Err(CwDexError::SlippageControlPriceFailed {
                        old_price,
                        new_price,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn get_min_out(&self) -> Option<Uint128> {
        match self {
            SlippageControl::MinOut(min_out) => Some(*min_out),
            _ => None,
        }
    }

    pub fn get_max_price_impact(&self) -> Option<Decimal> {
        match self {
            SlippageControl::MaxPriceImpact {
                max_price_impact,
            } => Some(*max_price_impact),
            _ => None,
        }
    }

    pub fn get_belief_price(&self) -> Option<Price> {
        match self {
            SlippageControl::BeliefPrice {
                belief_price,
                slippage_tolerance: _,
            } => Some(belief_price.clone()),
            _ => None,
        }
    }
}

impl Default for SlippageControl {
    fn default() -> Self {
        SlippageControl::MaxPriceImpact {
            max_price_impact: Decimal::percent(3),
        }
    }
}
