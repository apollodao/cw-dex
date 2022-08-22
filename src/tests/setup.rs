use std::marker::PhantomData;

use apollo_safe::pay::input::AddPaymentInput;
use apollo_safe::pay::msg::{PayInstantiateMsg, PayQueryMsg};
use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::OwnedDeps;
use cosmwasm_std::{
    from_binary,
    testing::{mock_env, mock_info},
    Decimal, Deps, Env, MessageInfo, Response, SubMsg, Uint128,
};
use cw_asset::AssetInfo;

use crate::{
    contract::{instantiate, query},
    state::{DistributionSchedule, PaymentInfo},
};

use super::custom_mock_querier::CustomMockQuerier;

pub struct InstantiateMsgBuilder {
    pub payments: Vec<AddPaymentInput>,
    pub cw3: String,
    pub fee: Decimal,
    pub distribution_contract: String,
}

// Builder pattern
impl InstantiateMsgBuilder {
    pub fn default() -> Self {
        Self {
            payments: vec![],
            fee: Decimal::from_ratio(1u8, 1000u16),
            cw3: "cw3".to_string(),
            distribution_contract: "distribution_contract".to_string(),
        }
    }

    pub fn with_initial_payments(mut self, payments: Option<Vec<AddPaymentInput>>) -> Self {
        if let Some(payments) = payments {
            self.payments = payments
        };
        self
    }

    pub fn with_fee(mut self, fee: Option<Decimal>) -> Self {
        if let Some(fee) = fee {
            self.fee = fee
        };
        self
    }

    pub fn build(self) -> PayInstantiateMsg {
        PayInstantiateMsg {
            payments: self.payments,
            cw3: "cw3".to_string(),
            apollo_safe_factory: "apollo_safe_factory".to_string(),
            distribution_contract: "distribution_contract".to_string(),
        }
    }
}

// this will set up the instantiation for other tests
pub fn do_instantiate() -> (OwnedDeps<MockStorage, MockApi, CustomMockQuerier>, MessageInfo) {
    _do_instantiate(None, None, None)
}

pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, CustomMockQuerier> {
    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: CustomMockQuerier::default(),
        custom_query_type: PhantomData,
    }
}

pub fn _do_instantiate(
    info: Option<MessageInfo>,
    payments: Option<Vec<AddPaymentInput>>,
    fee: Option<Decimal>,
) -> (OwnedDeps<MockStorage, MockApi, CustomMockQuerier>, MessageInfo) {
    let init_msg = InstantiateMsgBuilder::default()
        .with_initial_payments(payments.clone())
        .with_fee(fee)
        .build();

    // Funds admin with balances from MessageInfo
    let creator_info = match info {
        Some(info) => info,
        None => mock_info("creator", &[]),
    };

    let mut deps: OwnedDeps<MockStorage, MockApi, CustomMockQuerier> = mock_dependencies();
    // TODO: Make this for Vec<Coin>
    deps.querier.set_base_balances(creator_info.sender.as_str(), &creator_info.funds);

    if let Some(fee) = fee {
        deps.querier.set_raw_factory_fee(fee);
    };

    // deps.querier.set_cw20_balance("mock_token", "bob", 67890);
    // let info2 = AssetInfo::cw20(Addr::unchecked("mock_token"));
    // let balance2 = info2.query_balance(&deps.as_ref().querier, "bob").unwrap();
    // assert_eq!(balance2, Uint128::new(67890));

    // creator_info.funds[0].denom for now uusd
    let native_asset_info = AssetInfo::native("uusd");
    let balance = native_asset_info
        .query_balance(&deps.as_ref().querier, creator_info.sender.as_str())
        .unwrap();
    assert_eq!(balance, creator_info.funds[0].amount);

    let res = instantiate(deps.as_mut(), mock_env(), creator_info.clone(), init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let payments_resp: Vec<PaymentInfoResponse> = from_binary(
        &query(
            deps.as_ref(),
            mock_env(),
            PayQueryMsg::GetPayments {
                limit: None,
                start_after: None,
            },
        )
        .unwrap(),
    )
    .unwrap();

    match payments {
        Some(payments) => assert_eq!(payments.len(), payments_resp.len()),
        None => assert_eq!(0, payments_resp.len()),
    };
    (deps, creator_info)
}

// TODO: We should also increase heigth here since we return env
pub fn mock_env_time(time_delta: u64) -> Env {
    let mut env = mock_env();
    env.block.time = env.block.time.plus_seconds(time_delta);
    env
}

pub fn mock_env_time_minus(time_delta: u64) -> u64 {
    mock_env().block.time.minus_seconds(time_delta).seconds()
}

pub fn mock_env_time_plus(time_delta: u64) -> u64 {
    mock_env().block.time.plus_seconds(time_delta).seconds()
}

pub fn mock_add_payment(
    deps: Deps,
    params: AddPaymentInput,
    distribution_schedule: Option<DistributionSchedule>,
) -> PaymentInfo {
    let recipient = deps.api.addr_validate(&params.recipient).unwrap();
    let starting_time = params.starting_time.unwrap_or_else(|| mock_env().block.time.seconds());
    let ending_time = starting_time + params.distribution_time;

    match distribution_schedule {
        Some(distribution_schedule) => PaymentInfo::new(&recipient, distribution_schedule),
        None => {
            let asset = cw_asset::AssetBase {
                info: params.asset.parse(deps.api).unwrap(),
                amount: params.distribution_amount,
            };

            let distribution_schedule = DistributionSchedule {
                asset,
                starting_time,
                ending_time,
                claimed_amount: Uint128::zero(),
            };
            PaymentInfo::new(&recipient, distribution_schedule)
        }
    }

    // asset: payment.asset,
    // ending_time,
    // starting_time,
    // claimed: Uint128::zero(),
    // last_claimed: starting_time,
    // claimable_amount: Uint128::zero(),
}

pub fn get_amount_claimable(
    payment_info: &PaymentInfo,
    amount_to_claim: Option<Uint128>,
    fee: Decimal,
    time_delta: u64,
) -> (Uint128, Uint128) {
    let amount_claimable =
        payment_info.calculate_claimable_amount(&mock_env_time(time_delta)).unwrap();

    let calculated_amount = if let Some(amount_requested) = amount_to_claim {
        amount_requested.min(amount_claimable)
    } else {
        amount_claimable
    };

    let fee_amount = calculated_amount * fee;
    (calculated_amount, fee_amount)
}
pub fn mock_claim(
    mut payment_info: PaymentInfo,
    amount_to_claim: Option<Uint128>,
    time_delta: u64,
    fee: Decimal,
    distribution_contract: String,
) -> Vec<SubMsg> {
    let (calculated_amount, fee_amount) =
        get_amount_claimable(&payment_info, amount_to_claim, fee, time_delta);
    Response::new()
        .add_messages(
            payment_info
                .generate_claim_messages(distribution_contract, fee_amount, calculated_amount)
                .unwrap(),
        )
        .messages
}

pub fn mock_get_payment(payment_info: PaymentInfo, env: Env) -> DistributionScheduleResponse {
    payment_info.get_distribution_schedule(&env).unwrap()
}
