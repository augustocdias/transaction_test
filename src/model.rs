use std::collections::{HashMap, HashSet};

use actix::Message;
use bail_out::{ensure, ensure_not};
use rust_decimal::Decimal;

/// A transaction
#[derive(Deserialize)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Deserialize, Message)]
#[rtype(result = "Result<(), TransactionError>")]
pub struct Transaction {
    #[serde(rename = "type")]
    pub transaction_type: TransactionType,
    pub client: u16,
    pub tx: u32,
    #[serde(default)]
    pub amount: Option<Decimal>,
}

/// To store transaction history
#[derive(Clone)]
enum MoneyTransaction {
    Deposit(Decimal),
    Withdraw(Decimal),
}

impl MoneyTransaction {
    fn value(&self) -> &Decimal {
        match self {
            MoneyTransaction::Deposit(v) | MoneyTransaction::Withdraw(v) => v,
        }
    }
}

/// A message to instruct the actor to return the current account status of the actor
/// This will also instruct the system to stop the `AccountHandler` actor
#[derive(Message)]
#[rtype(result = "Account")]
pub struct Collect;

/// Possible errors for transactions' operations.
#[derive(Debug)]
pub enum TransactionError {
    InsufficientFunds,
    InvalidOperation,
    AccountLocked,
    TransactionAlreadyInDispute,
    TransactionNotInDispute,
    TransactionNotFound,
}

/// An entity containing a client's account values
#[derive(Serialize, Clone)]
pub struct Account {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
    #[serde(skip)]
    disputed: HashSet<u32>,
    #[serde(skip)]
    tx_history: HashMap<u32, MoneyTransaction>,
}

impl Account {
    /// Creates a new instance of an account.
    pub fn new(client: u16) -> Self {
        Self {
            client,
            available: Decimal::default(),
            held: Decimal::default(),
            total: Decimal::default(),
            locked: false,
            disputed: HashSet::new(),
            tx_history: HashMap::new(),
        }
    }

    /// Deposit funds
    ///
    /// # Errors
    /// If the account is locked, an error will be returned
    pub fn deposit(&mut self, value: Decimal, tx: u32) -> Result<(), TransactionError> {
        ensure_not!(self.locked, TransactionError::AccountLocked);
        self.available += value;
        self.tx_history.insert(tx, MoneyTransaction::Deposit(value));
        self.update_total_round();
        Ok(())
    }

    /// Withdraw funds
    ///
    /// # Errors
    /// If the account is locked or there's no available funds, an error will be returned
    pub fn withdraw(&mut self, value: Decimal, tx: u32) -> Result<(), TransactionError> {
        ensure_not!(self.locked, TransactionError::AccountLocked);
        ensure!(self.available >= value, TransactionError::InsufficientFunds);
        self.available -= value;
        self.tx_history
            .insert(tx, MoneyTransaction::Withdraw(value));
        self.update_total_round();
        Ok(())
    }

    /// Dispute funds
    ///
    /// # Errors
    /// If the account is locked, there's no available funds, the transaction is already in dispute,
    /// the origin transaction could not be found or the origin operation is not a deposit, an error
    /// will be returned
    pub fn dispute(&mut self, tx: u32) -> Result<(), TransactionError> {
        ensure_not!(self.locked, TransactionError::AccountLocked);
        ensure_not!(
            self.disputed.contains(&tx),
            TransactionError::TransactionAlreadyInDispute
        );
        let origin_tx = self
            .tx_history
            .get(&tx)
            .ok_or(TransactionError::TransactionNotFound)?;
        ensure!(
            matches!(origin_tx, MoneyTransaction::Deposit(_)),
            TransactionError::InvalidOperation
        );
        let value = origin_tx.value();
        ensure!(
            self.available >= *value,
            TransactionError::InsufficientFunds
        );
        self.available -= value;
        self.held += value;
        self.disputed.insert(tx);
        self.update_total_round();
        Ok(())
    }

    /// Resolves a dispute
    ///
    /// # Errors
    /// If the account is locked, the origin transaction is not in
    /// dispute or the origin transaction doesn't exist, an error will be returned
    pub fn resolve(&mut self, tx: u32) -> Result<(), TransactionError> {
        ensure_not!(self.locked, TransactionError::AccountLocked);
        let value = self
            .tx_history
            .get(&tx)
            .ok_or(TransactionError::TransactionNotFound)?
            .value();
        ensure!(
            self.disputed.contains(&tx),
            TransactionError::TransactionNotInDispute
        );
        assert!(self.held >= *value); // this should never happen, so panic
        self.available += value;
        self.held -= value;
        self.update_total_round();
        self.disputed.remove(&tx);
        Ok(())
    }

    /// Chargebacks a dispute. The account will be locked and no more transactions will be accepted
    ///
    /// # Errors
    /// If the account is locked, the origin transaction is not in
    /// dispute or the origin transaction doesn't exist, an error will be returned
    pub fn chargeback(&mut self, tx: u32) -> Result<(), TransactionError> {
        ensure_not!(self.locked, TransactionError::AccountLocked);
        let value = self
            .tx_history
            .get(&tx)
            .ok_or(TransactionError::TransactionNotFound)?
            .value();
        ensure!(
            self.disputed.contains(&tx),
            TransactionError::TransactionNotInDispute
        );
        assert!(self.held >= *value); // this should never happen, so panic
        self.held -= value;
        self.locked = true;
        self.update_total_round();
        self.disputed.remove(&tx);
        Ok(())
    }

    /// Updates the total value of the account and rounds the decimal numbers to 4 digits.
    /// Should be called after every transaction.
    fn update_total_round(&mut self) {
        self.total = self.held + self.available;
        self.total = self.total.round_dp(4);
        self.available = self.available.round_dp(4);
        self.held = self.held.round_dp(4);
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use crate::model::{Account, TransactionError};

    #[test]
    fn test_rounding() {
        let mut account = Account::new(1);
        account.deposit(dec!(140.12344), 2).unwrap();
        assert_eq!(account.total, dec!(140.1234));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(140.1234));
        account.deposit(dec!(100.00002), 1).unwrap();
        assert_eq!(account.total, dec!(240.1234));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(240.1234));
    }

    #[test]
    fn test_deposit() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        assert_eq!(account.total, dec!(240.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(240.26));
    }

    #[test]
    fn test_deposit_locked() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.dispute(1).unwrap();
        account.chargeback(1).unwrap();
        let err = account.deposit(dec!(140.14), 2).unwrap_err();
        assert!(matches!(err, TransactionError::AccountLocked));
        assert_eq!(account.total, dec!(0));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(0));
    }

    #[test]
    fn test_withdrawal() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.withdraw(dec!(40), 3).unwrap();
        assert_eq!(account.total, dec!(200.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(200.26));
    }

    #[test]
    fn test_withdrawal_locked() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.dispute(1).unwrap();
        account.chargeback(1).unwrap();
        let err = account.withdraw(dec!(140.14), 2).unwrap_err();
        assert!(matches!(err, TransactionError::AccountLocked));
        assert_eq!(account.total, dec!(0));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(0));
    }

    #[test]
    fn test_withdrawal_no_funds() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        let err = account.withdraw(dec!(340.14), 2).unwrap_err();
        assert!(matches!(err, TransactionError::InsufficientFunds));
        assert_eq!(account.total, dec!(240.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(240.26));
    }

    #[test]
    fn test_dispute() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.dispute(2).unwrap();
        account.withdraw(dec!(40.04), 3).unwrap();
        assert_eq!(account.total, dec!(200.22));
        assert_eq!(account.held, dec!(140.14));
        assert_eq!(account.available, dec!(60.08));
    }

    #[test]
    fn test_dispute_locked() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        account.chargeback(1).unwrap();
        let err = account.dispute(2).unwrap_err();
        assert!(matches!(err, TransactionError::AccountLocked));
        assert_eq!(account.total, dec!(200));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(200));
    }

    #[test]
    fn test_dispute_already_in_dispute() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.withdraw(dec!(40.04), 3).unwrap();
        account.dispute(2).unwrap();
        let err = account.dispute(2).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionAlreadyInDispute));
        assert_eq!(account.total, dec!(200.22));
        assert_eq!(account.held, dec!(140.14));
        assert_eq!(account.available, dec!(60.08));
    }

    #[test]
    fn test_dispute_tx_not_found() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        let err = account.dispute(3).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionNotFound));
        assert_eq!(account.total, dec!(240.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(240.26));
    }

    #[test]
    fn test_dispute_insufficient_funds() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.withdraw(dec!(200), 3).unwrap();
        let err = account.dispute(1).unwrap_err();
        assert!(matches!(err, TransactionError::InsufficientFunds));
        assert_eq!(account.total, dec!(40.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(40.26));
    }

    #[test]
    fn test_dispute_invalid_operation() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.withdraw(dec!(200), 3).unwrap();
        let err = account.dispute(3).unwrap_err();
        assert!(matches!(err, TransactionError::InvalidOperation));
        assert_eq!(account.total, dec!(40.26));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(40.26));
    }

    #[test]
    fn test_resolve() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.dispute(2).unwrap();
        account.withdraw(dec!(40.04), 3).unwrap();
        account.resolve(2).unwrap();
        assert_eq!(account.total, dec!(200.22));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(200.22));
    }

    #[test]
    fn test_resolve_locked() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        account.dispute(2).unwrap();
        account.chargeback(1).unwrap();
        let err = account.resolve(2).unwrap_err();
        assert!(matches!(err, TransactionError::AccountLocked));
        assert_eq!(account.total, dec!(200));
        assert_eq!(account.held, dec!(200));
        assert_eq!(account.available, dec!(0));
    }

    #[test]
    fn test_resolve_not_in_dispute() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        let err = account.resolve(2).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionNotInDispute));
        assert_eq!(account.total, dec!(300.12));
        assert_eq!(account.held, dec!(100.12));
        assert_eq!(account.available, dec!(200));
    }

    #[test]
    fn test_resolve_not_found() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        let err = account.resolve(4).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionNotFound));
        assert_eq!(account.total, dec!(300.12));
        assert_eq!(account.held, dec!(100.12));
        assert_eq!(account.available, dec!(200));
    }

    #[test]
    fn test_chargeback() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(140.14), 2).unwrap();
        account.dispute(2).unwrap();
        account.withdraw(dec!(40.04), 3).unwrap();
        account.chargeback(2).unwrap();
        assert_eq!(account.total, dec!(60.08));
        assert_eq!(account.held, dec!(0));
        assert_eq!(account.available, dec!(60.08));
        assert!(account.locked);
    }

    #[test]
    fn test_chargeback_locked() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        account.dispute(2).unwrap();
        account.chargeback(1).unwrap();
        let err = account.chargeback(2).unwrap_err();
        assert!(matches!(err, TransactionError::AccountLocked));
        assert_eq!(account.total, dec!(200));
        assert_eq!(account.held, dec!(200));
        assert_eq!(account.available, dec!(0));
        assert!(account.locked);
    }

    #[test]
    fn test_chargeback_not_in_dispute() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        let err = account.chargeback(2).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionNotInDispute));
        assert_eq!(account.total, dec!(300.12));
        assert_eq!(account.held, dec!(100.12));
        assert_eq!(account.available, dec!(200));
        assert!(!account.locked);
    }

    #[test]
    fn test_chargeback_not_found() {
        let mut account = Account::new(1);
        account.deposit(dec!(100.12), 1).unwrap();
        account.deposit(dec!(200), 2).unwrap();
        account.dispute(1).unwrap();
        let err = account.chargeback(4).unwrap_err();
        assert!(matches!(err, TransactionError::TransactionNotFound));
        assert_eq!(account.total, dec!(300.12));
        assert_eq!(account.held, dec!(100.12));
        assert_eq!(account.available, dec!(200));
        assert!(!account.locked);
    }
}
