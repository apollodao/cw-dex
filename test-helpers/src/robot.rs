use cosmwasm_std::{Coin, Uint128};
use cw_dex_test_contract::msg::{ExecuteMsg, OsmosisTestContractInstantiateMsg};
use cw_it::{
    helpers::{bank_balance_query, upload_wasm_file},
    osmosis_test_tube::{Module, OsmosisTestApp, SigningAccount, Wasm},
};

pub struct CwDexTestRobot<'a> {
    pub app: &'a OsmosisTestApp,
    pub test_contract_addr: String,
    pub pool_id: u64,
}

impl<'a> CwDexTestRobot<'a> {
    pub fn osmosis(
        app: &'a OsmosisTestApp,
        signer: &SigningAccount,
        init_msg: &OsmosisTestContractInstantiateMsg,
        wasm_file_path: &str,
    ) -> Self {
        // Upload test contract wasm file
        let code_id = upload_wasm_file(app, signer, wasm_file_path).unwrap();

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

    pub fn increase_time(&self, seconds: u64) -> &Self {
        self.app.increase_time(seconds);
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
