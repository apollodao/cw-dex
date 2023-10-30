use cosmwasm_std::{Coin, Uint128};
use cw_dex_test_contract::msg::{ExecuteMsg, OsmosisTestContractInstantiateMsg};
use cw_it::helpers::{bank_balance_query, upload_wasm_file};
use cw_it::test_tube::{Module, SigningAccount, Wasm};
use cw_it::traits::CwItRunner;

#[cfg(feature = "osmosis")]
use cw_it::ContractType;

#[cfg(feature = "osmosis")]
use cw_it::osmosis_test_tube::OsmosisTestApp;

pub struct CwDexTestRobot<'a, R>
where
    R: CwItRunner<'a>,
{
    pub app: &'a R,
    pub test_contract_addr: String,
    pub pool_id: u64,
}

#[cfg(feature = "osmosis")]
impl<'a> CwDexTestRobot<'a, OsmosisTestApp> {
    pub fn osmosis(
        app: &'a OsmosisTestApp,
        signer: &SigningAccount,
        init_msg: &OsmosisTestContractInstantiateMsg,
        contract: ContractType,
    ) -> Self {
        // Upload test contract wasm file
        let code_id = upload_wasm_file(app, signer, contract).unwrap();

        let wasm = Wasm::new(app);
        let test_contract_addr = wasm
            .instantiate(code_id, &init_msg, None, None, &[], signer)
            .unwrap()
            .data
            .address;

        Self {
            app,
            test_contract_addr,
            pool_id: init_msg.pool_id,
        }
    }

    pub fn increase_time(&self, seconds: u64) -> &Self {
        self.app.increase_time(seconds);
        self
    }
}

impl<'a, R> CwDexTestRobot<'a, R>
where
    R: CwItRunner<'a>,
{
    pub fn assert_native_balance(
        &self,
        address: String,
        denom: String,
        expected: impl Into<Uint128>,
    ) -> &Self {
        let balance = bank_balance_query(self.app, address, denom).unwrap();
        assert_eq!(balance, expected.into());
        self
    }

    pub fn assert_lp_balance(&self, address: String, expected: impl Into<Uint128>) -> &Self {
        self.assert_native_balance(address, format!("gamm/pool/{}", self.pool_id), expected);
        self
    }

    pub fn stake(&self, signer: &SigningAccount, amount: Uint128) -> &Self {
        let wasm = Wasm::new(self.app);
        wasm.execute(
            &self.test_contract_addr,
            &ExecuteMsg::Stake { amount },
            &[Coin {
                amount,
                denom: format!("gamm/pool/{}", self.pool_id),
            }],
            signer,
        )
        .unwrap();
        self
    }

    pub fn unlock(&self, signer: &SigningAccount, amount: Uint128) -> &Self {
        let wasm = Wasm::new(self.app);
        wasm.execute(
            &self.test_contract_addr,
            &ExecuteMsg::Unlock { amount },
            &[],
            signer,
        )
        .unwrap();
        self
    }

    pub fn superfluid_stake(&self, signer: &SigningAccount, amount: Uint128) -> &Self {
        let wasm = Wasm::new(self.app);
        wasm.execute(
            &self.test_contract_addr,
            &ExecuteMsg::SuperfluidStake { amount },
            &[Coin {
                amount,
                denom: format!("gamm/pool/{}", self.pool_id),
            }],
            signer,
        )
        .unwrap();
        self
    }

    pub fn superfluid_unlock(&self, signer: &SigningAccount, amount: Uint128) -> &Self {
        let wasm = Wasm::new(self.app);
        wasm.execute(
            &self.test_contract_addr,
            &ExecuteMsg::SuperfluidUnlock { amount },
            &[],
            signer,
        )
        .unwrap();
        self
    }

    pub fn withdraw_unlocked(&self, signer: &SigningAccount, amount: Uint128) -> &Self {
        let wasm = Wasm::new(self.app);
        wasm.execute(
            &self.test_contract_addr,
            &ExecuteMsg::WithdrawUnlocked { amount },
            &[],
            signer,
        )
        .unwrap();
        self
    }
}
