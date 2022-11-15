use std::{str::FromStr, time::Duration};

use apollo_utils::iterators::TryIntoElementwise;
use cosmwasm_std::{Coin, StdError, StdResult, Uint128};
use prost::Message;

pub(crate) trait ToProtobufDuration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration;
}

impl ToProtobufDuration for Duration {
    fn to_protobuf_duration(&self) -> osmosis_std::shim::Duration {
        osmosis_std::shim::Duration {
            seconds: self.as_secs() as i64,
            nanos: self.subsec_nanos() as i32,
        }
    }
}

pub struct BalancerPoolAsset {
    pub token: Coin,
    pub weight: Uint128,
}

impl TryFrom<osmosis_std::types::osmosis::gamm::v1beta1::PoolAsset> for BalancerPoolAsset {
    type Error = StdError;

    fn try_from(
        value: osmosis_std::types::osmosis::gamm::v1beta1::PoolAsset,
    ) -> Result<Self, Self::Error> {
        let token = value
            .token
            .ok_or_else(|| StdError::generic_err("BalancerPoolAsset::try_from: token is None"))?;
        Ok(BalancerPoolAsset {
            token: Coin {
                denom: token.denom,
                amount: Uint128::from_str(&token.amount)?,
            },
            weight: Uint128::from_str(&value.weight)?,
        })
    }
}

pub struct BalancerPoolAssets(Vec<BalancerPoolAsset>);

impl From<Vec<BalancerPoolAsset>> for BalancerPoolAssets {
    fn from(value: Vec<BalancerPoolAsset>) -> Self {
        BalancerPoolAssets(value)
    }
}

impl TryFrom<Vec<osmosis_std::types::osmosis::gamm::v1beta1::PoolAsset>> for BalancerPoolAssets {
    type Error = StdError;

    fn try_from(
        value: Vec<osmosis_std::types::osmosis::gamm::v1beta1::PoolAsset>,
    ) -> Result<Self, Self::Error> {
        let pool_assets: Vec<BalancerPoolAsset> = value.try_into_elementwise()?;
        Ok(Self(pool_assets))
    }
}

impl BalancerPoolAssets {
    pub fn get_pool_weight(&self, denom: &str) -> StdResult<Uint128> {
        self.0
            .iter()
            .find(|pool_asset| pool_asset.token.denom == denom)
            .map(|pool_asset| pool_asset.weight)
            .ok_or_else(|| {
                StdError::generic_err("BalancerPoolAssets::get_pool_weight: pool asset not found")
            })
    }
}

pub enum SupportedPoolType {
    Balancer(osmosis_std::types::osmosis::gamm::v1beta1::Pool),
    StableSwap(osmosis_std::types::osmosis::gamm::poolmodels::stableswap::v1beta1::Pool),
}

impl TryFrom<osmosis_std::shim::Any> for SupportedPoolType {
    type Error = StdError;

    fn try_from(value: osmosis_std::shim::Any) -> Result<Self, Self::Error> {
        if let Ok(pool) =
            osmosis_std::types::osmosis::gamm::v1beta1::Pool::decode(value.value.as_slice())
        {
            return Ok(SupportedPoolType::Balancer(pool));
        }
        if let Ok(pool) =
            osmosis_std::types::osmosis::gamm::poolmodels::stableswap::v1beta1::Pool::decode(
                value.value.as_slice(),
            )
        {
            return Ok(SupportedPoolType::StableSwap(pool));
        }

        Err(StdError::ParseErr {
            target_type: "Pool".to_string(),
            msg: "Unmatched pool: must be either `Balancer` or `StableSwap`.".to_string(),
        })
    }
}
