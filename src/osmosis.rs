use std::convert::TryFrom;

use apollo_proto_rust::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgSwapExactAmountIn, SwapAmountInRoute,
};
use apollo_proto_rust::utils::encode;
use apollo_proto_rust::OsmosisTypeURLs;
use cosmwasm_std::{Addr, Coin, CosmosMsg, StdError, StdResult};
use cw_asset::osmosis::OsmosisCoin;
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::{CwDexError, Pool};

pub struct OsmosisPool {
    pool_id: u64,
}

impl Pool for OsmosisPool {
    fn provide_liquidity(
        &self,
        assets: AssetList,
        sender: Option<Addr>,
    ) -> Result<CosmosMsg, CwDexError> {
        let coins = assets
            .into_iter()
            .map(|asset| OsmosisCoin::try_from(asset.clone()))
            .collect::<StdResult<Vec<OsmosisCoin>>>()?;

        let sender = match sender {
            Some(addr) => Ok(addr.to_string()),
            None => Err(CwDexError::Std(StdError::generic_err("osmosis error: no sender"))),
        }?;

        let join_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::JoinPool.to_string(),
            value: encode(MsgJoinPool {
                pool_id: self.pool_id,
                sender,
                share_out_amount: todo!(),
                token_in_maxs: coins
                    .iter()
                    .map(|coin| coin.0.clone().into())
                    .collect::<Vec<apollo_proto_rust::cosmos::base::v1beta1::Coin>>(),
            }),
        };

        Ok(join_msg)
    }

    fn withdraw_liquidity(
        &self,
        asset: Asset,
        sender: Option<Addr>,
    ) -> Result<CosmosMsg, CwDexError> {
        let sender = match sender {
            Some(addr) => Ok(addr.to_string()),
            None => Err(CwDexError::Std(StdError::generic_err("osmosis error: no sender"))),
        }?;

        let exit_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::ExitPool.to_string(),
            value: encode(MsgExitPool {
                sender,
                pool_id: self.pool_id,
                share_in_amount: asset.amount.to_string(),
                token_out_mins: todo!(),
            }),
        };

        Ok(exit_msg)
    }

    fn swap_msg(&self, offer: Asset, ask: Asset, sender: Addr) -> Result<CosmosMsg, CwDexError> {
        let out_denom = match ask.info {
            AssetInfo::Cw20(_) => Err(CwDexError::InvalidOutAsset {}),
            AssetInfo::Native(denom) => Ok(denom),
        }?;

        let swap_msg = CosmosMsg::Stargate {
            type_url: OsmosisTypeURLs::SwapExactAmountIn.to_string(),
            value: encode(MsgSwapExactAmountIn {
                sender: sender.to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id: self.pool_id,
                    token_out_denom: out_denom,
                }],
                token_in: Some(Coin::try_from(offer)?.into()),
                token_out_min_amount: ask.amount.to_string(),
            }),
        };

        Ok(swap_msg)
    }
}
