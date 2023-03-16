use core::ops::{
    Add,
    Div,
    Mul,
};

use crate::traits::types::WrappedU256;
pub use crate::traits::{
    controller::ControllerRef,
    interest_rate_model::InterestRateModelRef,
    pool::*,
};
use ink::{
    prelude::vec::Vec,
    LangError,
};
use openbrush::{
    contracts::psp22::{
        self,
        Internal as PSP22Internal,
        PSP22Ref,
    },
    storage::Mapping,
    traits::{
        AccountId,
        Balance,
        Storage,
        Timestamp,
        ZERO_ADDRESS,
    },
};
use primitive_types::U256;

use super::exp_no_err::Exp;

pub const STORAGE_KEY: u32 = openbrush::storage_unique_key!(Data);

#[derive(Debug, scale::Decode, scale::Encode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct BorrowSnapshot {
    principal: Balance,
    interest_index: WrappedU256,
}

struct CalculateInterestInput {
    total_borrows: Balance,
    total_reserves: Balance,
    borrow_index: Exp,
    borrow_rate: U256,
    old_block_timestamp: Timestamp,
    new_block_timestamp: Timestamp,
    reserve_factor: U256,
}

struct CalculateInterestOutput {
    borrow_index: Exp,
    total_borrows: Balance,
    total_reserves: Balance,
    interest_accumulated: Balance,
}

fn borrow_rate_max_mantissa() -> U256 {
    // .0005% / time
    U256::from(10)
        .pow(U256::from(16))
        .mul(U256::from(5))
        .div(U256::from(1000))
}

fn calculate_interest(input: &CalculateInterestInput) -> CalculateInterestOutput {
    if input.borrow_rate.gt(&borrow_rate_max_mantissa()) {
        panic!("borrow rate is absurdly high")
    }
    let delta = input
        .new_block_timestamp
        .abs_diff(input.old_block_timestamp);
    let simple_interest_factor = Exp {
        mantissa: WrappedU256::from(input.borrow_rate),
    }
    .mul_mantissa(U256::from(delta));

    let interest_accumulated =
        simple_interest_factor.mul_scalar_truncate(U256::from(input.total_borrows));

    let total_borrows_new = interest_accumulated.as_u128().add(input.total_borrows);
    let total_reserves_new = Exp {
        mantissa: WrappedU256::from(input.reserve_factor),
    }
    .mul_scalar_truncate_add_uint(interest_accumulated, U256::from(input.total_reserves));
    let borrow_index_new = simple_interest_factor.mul_scalar_truncate_add_uint(
        input.borrow_index.mantissa.into(),
        input.borrow_index.mantissa.into(),
    );
    CalculateInterestOutput {
        borrow_index: Exp {
            mantissa: WrappedU256::from(borrow_index_new),
        },
        interest_accumulated: interest_accumulated.as_u128(),
        total_borrows: total_borrows_new,
        total_reserves: total_reserves_new.as_u128(), // TODO
    }
}
#[derive(Debug)]
#[openbrush::upgradeable_storage(STORAGE_KEY)]
pub struct Data {
    pub underlying: AccountId,
    pub controller: AccountId,
    pub rate_model: AccountId,
    pub total_borrows: Balance,
    pub total_reserves: Balance,
    pub account_borrows: Mapping<AccountId, BorrowSnapshot>,
    pub accural_block_timestamp: Timestamp,
    pub borrow_index: WrappedU256,
    pub reserve_factor: WrappedU256,
}

impl Default for Data {
    fn default() -> Self {
        Data {
            underlying: ZERO_ADDRESS.into(),
            controller: ZERO_ADDRESS.into(),
            rate_model: ZERO_ADDRESS.into(),
            total_borrows: Default::default(),
            total_reserves: Default::default(),
            account_borrows: Default::default(),
            accural_block_timestamp: 0,
            borrow_index: WrappedU256::from(U256::zero()),
            reserve_factor: WrappedU256::from(U256::zero()),
        }
    }
}

pub trait Internal {
    fn _accrue_interest(&mut self);
    fn _accure_interest_at(&mut self, at: Timestamp);
    fn _mint(&mut self, minter: AccountId, mint_amount: Balance) -> Result<()>;
    fn _redeem(
        &mut self,
        redeemer: AccountId,
        redeem_tokens_in: Balance,
        redeem_amount_in: Balance,
    ) -> Result<()>;
    fn _borrow(&mut self, borrower: AccountId, borrow_amount: Balance) -> Result<()>;
    fn _repay_borrow(
        &mut self,
        payer: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
    ) -> Result<Balance>;
    fn _liquidate_borrow(
        &mut self,
        liquidator: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
        collateral: AccountId,
    ) -> Result<()>;
    fn _seize(
        &mut self,
        seizer_token: AccountId,
        liquidator: AccountId,
        borrower: AccountId,
        seize_tokens: Balance,
    ) -> Result<()>;

    fn _transfer_underlying_from(
        &self,
        from: AccountId,
        to: AccountId,
        value: Balance,
    ) -> Result<()>;
    fn _transfer_underlying(&self, to: AccountId, value: Balance) -> Result<()>;

    fn _underlying(&self) -> AccountId;
    fn _controller(&self) -> AccountId;
    fn _get_cash_prior(&self) -> Balance;
    fn _total_borrows(&self) -> Balance;
    fn _total_reserves(&self) -> Balance;
    fn _rate_model(&self) -> AccountId;
    fn _borrow_balance_stored(&self, account: AccountId) -> Balance;
    fn _accural_block_timestamp(&self) -> Timestamp;
    fn _borrow_index(&self) -> Exp;
    fn _reserve_factor(&self) -> Exp;

    // event emission
    fn _emit_mint_event(&self, minter: AccountId, mint_amount: Balance, mint_tokens: Balance);
    fn _emit_redeem_event(
        &self,
        redeemer: AccountId,
        redeem_amount: Balance,
        redeem_tokens: Balance,
    );
    fn _emit_borrow_event(
        &self,
        borrower: AccountId,
        borrow_amount: Balance,
        account_borrows: Balance,
        total_borrows: Balance,
    );
    fn _emit_repay_borrow_event(
        &self,
        payer: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
        account_borrows: Balance,
        total_borrows: Balance,
    );
    fn _emit_liquidate_borrow_event(
        &self,
        liquidator: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
        token_collateral: AccountId,
        seize_tokens: Balance,
    );
    fn _emit_reserves_added_event(
        &self,
        benefactor: AccountId,
        add_amount: Balance,
        new_total_reserves: Balance,
    );
    fn _emit_accrue_interest_event(
        &self,
        interest_accumulated: Balance,
        new_index: WrappedU256,
        new_total_borrows: Balance,
    );
}

impl<T: Storage<Data> + Storage<psp22::Data>> Pool for T {
    default fn mint(&mut self, mint_amount: Balance) -> Result<()> {
        self._accrue_interest();
        self._mint(Self::env().caller(), mint_amount)
    }

    default fn get_accrual_block_timestamp(&self) -> Timestamp {
        self._accural_block_timestamp()
    }

    default fn redeem(&mut self, redeem_tokens: Balance) -> Result<()> {
        self._accrue_interest();
        self._redeem(Self::env().caller(), redeem_tokens, 0)
    }

    default fn redeem_underlying(&mut self, redeem_amount: Balance) -> Result<()> {
        self._accrue_interest();
        self._redeem(Self::env().caller(), 0, redeem_amount)
    }

    default fn borrow(&mut self, borrow_amount: Balance) -> Result<()> {
        self._accrue_interest();
        self._borrow(Self::env().caller(), borrow_amount)
    }

    default fn repay_borrow(&mut self, repay_amount: Balance) -> Result<()> {
        self._accrue_interest();
        self._repay_borrow(Self::env().caller(), Self::env().caller(), repay_amount)?;
        Ok(())
    }

    default fn repay_borrow_behalf(
        &mut self,
        borrower: AccountId,
        repay_amount: Balance,
    ) -> Result<()> {
        self._accrue_interest();
        self._repay_borrow(Self::env().caller(), borrower, repay_amount)?;
        Ok(())
    }

    default fn liquidate_borrow(
        &mut self,
        borrower: AccountId,
        repay_amount: Balance,
        collateral: AccountId,
    ) -> Result<()> {
        self._accrue_interest();
        self._liquidate_borrow(Self::env().caller(), borrower, repay_amount, collateral)
    }

    default fn seize(
        &mut self,
        liquidator: AccountId,
        borrower: AccountId,
        seize_tokens: Balance,
    ) -> Result<()> {
        self._accrue_interest();
        self._seize(Self::env().caller(), liquidator, borrower, seize_tokens)
    }

    default fn underlying(&self) -> AccountId {
        self._underlying()
    }

    default fn controller(&self) -> AccountId {
        self._controller()
    }

    default fn get_cash_prior(&self) -> Balance {
        self._get_cash_prior()
    }

    default fn total_borrows(&self) -> Balance {
        self._total_borrows()
    }

    default fn borrow_balance_stored(&self, account: AccountId) -> Balance {
        self._borrow_balance_stored(account)
    }
}

impl<T: Storage<Data> + Storage<psp22::Data>> Internal for T {
    default fn _accrue_interest(&mut self) {
        self._accure_interest_at(Self::env().block_timestamp())
    }
    default fn _accure_interest_at(&mut self, at: Timestamp) {
        let accural = self._accural_block_timestamp();
        if accural.eq(&at) {
            return
        }
        let balance = PSP22Ref::balance_of(&self._underlying(), Self::env().account_id());
        let borrows = self._total_borrows();
        let reserves = self._total_reserves();
        let idx = self._borrow_index();
        let borrow_rate =
            InterestRateModelRef::get_borrow_rate(&self._rate_model(), balance, borrows, reserves);
        let out = calculate_interest(&CalculateInterestInput {
            total_borrows: borrows,
            total_reserves: reserves,
            borrow_index: idx,
            borrow_rate: borrow_rate.into(),
            old_block_timestamp: self._accural_block_timestamp(),
            new_block_timestamp: at,
            reserve_factor: self._reserve_factor().mantissa.into(),
        });
        let mut data = self.data::<Data>();
        data.accural_block_timestamp = at;
        data.borrow_index = out.borrow_index.mantissa;
        data.total_borrows = out.total_borrows;
        data.total_reserves = out.total_reserves;
        self._emit_accrue_interest_event(
            out.interest_accumulated,
            WrappedU256::from(out.borrow_index.mantissa),
            out.total_borrows,
        )
    }
    default fn _mint(&mut self, minter: AccountId, mint_amount: Balance) -> Result<()> {
        let contract_addr = Self::env().account_id();
        ControllerRef::mint_allowed(&self._controller(), contract_addr, minter, mint_amount)
            .unwrap();

        let current_timestamp = Self::env().block_timestamp();
        if self._accural_block_timestamp() != current_timestamp {
            return Err(Error::AccrualBlockNumberIsNotFresh)
        };
        // TODO: calculate exchange rate & mint amount
        let actual_mint_amount = mint_amount;
        self._transfer_underlying_from(minter, contract_addr, actual_mint_amount)
            .unwrap();
        self._mint_to(minter, mint_amount).unwrap();

        self._emit_mint_event(minter, actual_mint_amount, mint_amount);

        Ok(())
    }
    default fn _redeem(
        &mut self,
        redeemer: AccountId,
        redeem_tokens_in: Balance,
        redeem_amount_in: Balance,
    ) -> Result<()> {
        let exchange_rate = 1; // TODO: calculate exchange rate & redeem amount
        let (redeem_tokens, redeem_amount) = match (redeem_tokens_in, redeem_amount_in) {
            (tokens, _) if tokens > 0 => (tokens, tokens * exchange_rate),
            (_, amount) if amount > 0 => (amount / exchange_rate, amount),
            _ => return Err(Error::InvalidParameter),
        };

        let contract_addr = Self::env().account_id();
        ControllerRef::redeem_allowed(&self._controller(), contract_addr, redeemer, redeem_tokens)
            .unwrap();

        if self._get_cash_prior() < redeem_amount {
            return Err(Error::RedeemTransferOutNotPossible)
        }

        self._burn_from(redeemer, redeem_tokens).unwrap();
        self._transfer_underlying(redeemer, redeem_amount).unwrap();

        self._emit_redeem_event(redeemer, redeem_amount, redeem_tokens);

        Ok(())
    }
    default fn _borrow(&mut self, borrower: AccountId, borrow_amount: Balance) -> Result<()> {
        let contract_addr = Self::env().account_id();
        ControllerRef::borrow_allowed(&self._controller(), contract_addr, borrower, borrow_amount)
            .unwrap();

        let current_timestamp = Self::env().block_timestamp();
        if self._accural_block_timestamp() != current_timestamp {
            return Err(Error::AccrualBlockNumberIsNotFresh)
        };
        if self._get_cash_prior() < borrow_amount {
            return Err(Error::BorrowCashNotAvailable)
        }

        let account_borrows_prev = self._borrow_balance_stored(borrower);
        let account_borrows_new = account_borrows_prev + borrow_amount;
        let total_borrows_new = self._total_borrows() + borrow_amount;
        let idx = self._borrow_index().mantissa;
        self.data::<Data>().account_borrows.insert(
            &borrower,
            &BorrowSnapshot {
                principal: account_borrows_new,
                interest_index: idx,
            },
        );
        self.data::<Data>().total_borrows = total_borrows_new;

        self._transfer_underlying(borrower, borrow_amount).unwrap();

        self._emit_borrow_event(
            borrower,
            borrow_amount,
            account_borrows_new,
            total_borrows_new,
        );

        Ok(())
    }

    default fn _repay_borrow(
        &mut self,
        payer: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
    ) -> Result<Balance> {
        let contract_addr = Self::env().account_id();
        ControllerRef::repay_borrow_allowed(
            &self._controller(),
            contract_addr,
            payer,
            borrower,
            repay_amount,
        )
        .unwrap();

        let current_timestamp = Self::env().block_timestamp();
        if self._accural_block_timestamp() != current_timestamp {
            return Err(Error::AccrualBlockNumberIsNotFresh)
        };

        let account_borrow_prev = self._borrow_balance_stored(borrower);
        let repay_amount_final = if repay_amount == u128::MAX {
            account_borrow_prev
        } else {
            repay_amount
        };

        self._transfer_underlying_from(payer, contract_addr, repay_amount_final)
            .unwrap();

        let account_borrows_new = account_borrow_prev - repay_amount_final;
        let total_borrows_new = self._total_borrows() - repay_amount_final;

        let idx = self._borrow_index().mantissa;
        self.data::<Data>().account_borrows.insert(
            &borrower,
            &BorrowSnapshot {
                principal: account_borrows_new,
                interest_index: idx,
            },
        );
        self.data::<Data>().total_borrows = total_borrows_new;

        self._emit_repay_borrow_event(
            payer,
            borrower,
            repay_amount_final,
            account_borrows_new,
            total_borrows_new,
        );

        Ok(repay_amount_final)
    }
    default fn _liquidate_borrow(
        &mut self,
        liquidator: AccountId,
        borrower: AccountId,
        repay_amount: Balance,
        collateral: AccountId,
    ) -> Result<()> {
        let contract_addr = Self::env().account_id();
        ControllerRef::liquidate_borrow_allowed(
            &self._controller(),
            contract_addr,
            collateral,
            liquidator,
            borrower,
            repay_amount,
        )
        .unwrap();

        let current_timestamp = Self::env().block_timestamp();
        if self._accural_block_timestamp() != current_timestamp {
            return Err(Error::AccrualBlockNumberIsNotFresh)
        }
        if PoolRef::get_accrual_block_timestamp(&collateral) != current_timestamp {
            return Err(Error::AccrualBlockNumberIsNotFresh)
        }

        if liquidator == borrower {
            return Err(Error::LiquidateLiquidatorIsBorrower)
        }
        if repay_amount == 0 {
            return Err(Error::LiquidateCloseAmountIsZero)
        }

        let actual_repay_amount = self
            ._repay_borrow(liquidator, borrower, repay_amount)
            .unwrap();

        // seize
        if collateral == contract_addr {
            self._seize(contract_addr, liquidator, borrower, 0).unwrap(); // TODO: seize_token's amount (seize_tokens) calculated
        } else {
            PoolRef::seize(&collateral, liquidator, borrower, 0).unwrap(); // TODO: seize_token's amount (seize_tokens) calculated
        }

        self._emit_liquidate_borrow_event(liquidator, borrower, actual_repay_amount, collateral, 0);

        Ok(())
    }
    default fn _seize(
        &mut self,
        seizer_token: AccountId,
        liquidator: AccountId,
        borrower: AccountId,
        seize_tokens: Balance,
    ) -> Result<()> {
        let contract_addr = Self::env().account_id();
        ControllerRef::seize_allowed(
            &self._controller(),
            contract_addr,
            seizer_token,
            liquidator,
            borrower,
            seize_tokens,
        )
        .unwrap();

        if liquidator == borrower {
            return Err(Error::LiquidateSeizeLiquidatorIsBorrower)
        }

        // calculate the new borrower and liquidator token balances
        let protocol_seize_token = 0; // TODO
        let liquidator_seize_token = seize_tokens - protocol_seize_token;
        let exchange_rate = 1; // TODO
        let protocol_seize_amount = protocol_seize_token * exchange_rate;
        let total_reserves_new = self._total_reserves() + protocol_seize_amount;

        // EFFECTS & INTERACTIONS
        self.data::<Data>().total_reserves = total_reserves_new;
        // total_supply = total_supply - protocol_seize_token; // TODO: check
        self._burn_from(borrower, seize_tokens).unwrap();
        self._mint_to(liquidator, liquidator_seize_token).unwrap();

        self._emit_reserves_added_event(contract_addr, protocol_seize_amount, total_reserves_new);

        Ok(())
    }

    fn _transfer_underlying_from(
        &self,
        from: AccountId,
        to: AccountId,
        value: Balance,
    ) -> Result<()> {
        PSP22Ref::transfer_from_builder(&self._underlying(), from, to, value, Vec::<u8>::new())
            .call_flags(ink::env::CallFlags::default().set_allow_reentry(true))
            .try_invoke()
            .unwrap()
            .unwrap()
            .map_err(to_psp22_error)
    }
    fn _transfer_underlying(&self, to: AccountId, value: Balance) -> Result<()> {
        PSP22Ref::transfer(&self._underlying(), to, value, Vec::<u8>::new()).map_err(to_psp22_error)
    }

    default fn _underlying(&self) -> AccountId {
        self.data::<Data>().underlying
    }

    default fn _controller(&self) -> AccountId {
        self.data::<Data>().controller
    }

    default fn _get_cash_prior(&self) -> Balance {
        PSP22Ref::balance_of(&self._underlying(), Self::env().account_id())
    }

    default fn _total_borrows(&self) -> Balance {
        self.data::<Data>().total_borrows
    }

    default fn _rate_model(&self) -> AccountId {
        self.data::<Data>().rate_model
    }

    default fn _accural_block_timestamp(&self) -> Timestamp {
        Timestamp::from(self.data::<Data>().accural_block_timestamp)
    }

    default fn _total_reserves(&self) -> Balance {
        self.data::<Data>().total_reserves
    }

    default fn _borrow_index(&self) -> Exp {
        Exp {
            mantissa: self.data::<Data>().borrow_index,
        }
    }

    default fn _borrow_balance_stored(&self, account: AccountId) -> Balance {
        let snapshot = match self.data::<Data>().account_borrows.get(&account) {
            Some(value) => value,
            None => return 0,
        };

        if snapshot.principal == 0 {
            return 0
        }
        let borrow_index = self._borrow_index();
        let prinicipal_times_index =
            U256::from(snapshot.principal).mul(U256::from(borrow_index.mantissa));
        prinicipal_times_index
            .div(U256::from(snapshot.interest_index))
            .as_u128()
    }

    default fn _reserve_factor(&self) -> Exp {
        Exp {
            mantissa: self.data::<Data>().reserve_factor,
        }
    }

    // event emission
    default fn _emit_mint_event(
        &self,
        _minter: AccountId,
        _mint_amount: Balance,
        _mint_tokens: Balance,
    ) {
    }
    default fn _emit_redeem_event(
        &self,
        _redeemer: AccountId,
        _redeem_amount: Balance,
        _redeem_tokens: Balance,
    ) {
    }
    default fn _emit_borrow_event(
        &self,
        _borrower: AccountId,
        _borrow_amount: Balance,
        _account_borrows: Balance,
        _total_borrows: Balance,
    ) {
    }
    default fn _emit_repay_borrow_event(
        &self,
        _payer: AccountId,
        _borrower: AccountId,
        _repay_amount: Balance,
        _account_borrows: Balance,
        _total_borrows: Balance,
    ) {
    }
    default fn _emit_liquidate_borrow_event(
        &self,
        _liquidator: AccountId,
        _borrower: AccountId,
        _repay_amount: Balance,
        _token_collateral: AccountId,
        _seize_tokens: Balance,
    ) {
    }

    default fn _emit_reserves_added_event(
        &self,
        _benefactor: AccountId,
        _add_amount: Balance,
        _new_total_reserves: Balance,
    ) {
    }

    default fn _emit_accrue_interest_event(
        &self,
        _interest_accumulated: Balance,
        _new_index: WrappedU256,
        _new_total_borrows: Balance,
    ) {
    }
}

pub fn to_psp22_error(e: psp22::PSP22Error) -> Error {
    Error::PSP22(e)
}

pub fn to_lang_error(e: LangError) -> Error {
    Error::Lang(e)
}

#[cfg(test)]
mod tests {
    use super::Exp;

    use super::*;
    use primitive_types::U256;
    fn mantissa() -> U256 {
        U256::from(10).pow(U256::from(18))
    }
    #[test]
    #[should_panic(expected = "borrow rate is absurdly high")]
    fn test_calculate_interest_panic_if_over_borrow_rate_max() {
        let input = CalculateInterestInput {
            borrow_index: Exp {
                mantissa: WrappedU256::from(U256::zero()),
            },
            borrow_rate: U256::one().mul(U256::from(10)).pow(U256::from(18)),
            new_block_timestamp: Timestamp::default(),
            old_block_timestamp: Timestamp::default(),
            reserve_factor: U256::zero(),
            total_borrows: Balance::default(),
            total_reserves: Balance::default(),
        };
        calculate_interest(&input);
    }

    #[test]
    fn test_calculate_interest() {
        let old_timestamp = Timestamp::default();
        let inputs: &[CalculateInterestInput] = &[
            CalculateInterestInput {
                old_block_timestamp: old_timestamp,
                new_block_timestamp: old_timestamp + mantissa().as_u64(),
                borrow_index: Exp {
                    mantissa: WrappedU256::from(U256::zero()),
                },
                borrow_rate: mantissa().div(100000), // 0.001 %
                reserve_factor: mantissa().div(100), // 1 %
                total_borrows: 10_000 * (10_u128.pow(18)),
                total_reserves: 10_000 * (10_u128.pow(18)),
            },
            CalculateInterestInput {
                old_block_timestamp: old_timestamp,
                new_block_timestamp: old_timestamp + 1000 * 60 * 60, // 1 hour
                borrow_index: Exp {
                    mantissa: WrappedU256::from(U256::from(123123123)),
                },
                borrow_rate: mantissa().div(1000000),
                reserve_factor: mantissa().div(10),
                total_borrows: 100_000 * (10_u128.pow(18)),
                total_reserves: 1_000_000 * (10_u128.pow(18)),
            },
            CalculateInterestInput {
                old_block_timestamp: old_timestamp,
                new_block_timestamp: old_timestamp + 999 * 60 * 60 * 2345 * 123,
                borrow_index: Exp {
                    mantissa: WrappedU256::from(U256::from(123123123)),
                },
                borrow_rate: mantissa().div(123123),
                reserve_factor: mantissa().div(10).mul(2),
                total_borrows: 123_456 * (10_u128.pow(18)),
                total_reserves: 789_012 * (10_u128.pow(18)),
            },
        ];
        for input in inputs {
            let got = calculate_interest(&input);
            let delta = input
                .new_block_timestamp
                .abs_diff(input.old_block_timestamp);
            // interest accumulated should be (borrow rate * delta * total borrows)
            let interest_want = input
                .borrow_rate
                .mul(U256::from(
                    input.new_block_timestamp - input.old_block_timestamp,
                ))
                .mul(U256::from(input.total_borrows))
                .div(mantissa())
                .as_u128();
            let reserves_want = U256::from(input.reserve_factor)
                .mul(U256::from(interest_want))
                .div(U256::from(10_u128.pow(18)))
                .add(U256::from(input.total_reserves));
            assert_eq!(got.interest_accumulated, interest_want);
            assert_eq!(got.total_borrows, interest_want + (input.total_borrows));
            assert_eq!(got.total_reserves, reserves_want.as_u128());
            let borrow_idx_want = input
                .borrow_rate
                .mul(U256::from(delta))
                .mul(U256::from(input.borrow_index.mantissa))
                .div(U256::from(10_u128.pow(18)))
                .add(U256::from(input.borrow_index.mantissa));
            assert_eq!(U256::from(got.borrow_index.mantissa), borrow_idx_want);
        }
    }
}
