use actix::{Actor, ActorContext, Addr, Context, Handler, MessageResult, Supervised, Supervisor};
use log::info;

use crate::model::{Account, Collect, Transaction, TransactionError, TransactionType};

/// Actor to hold the state of each client's account
pub struct AccountHandler {
    client: u16,
    account: Account,
}

impl AccountHandler {
    /// Creates a new account and starts the actor
    pub fn new(client_id: u16) -> Addr<Self> {
        Supervisor::start(move |_| Self {
            client: client_id,
            account: Account::new(client_id),
        })
    }
}

impl Actor for AccountHandler {
    type Context = Context<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        info!("Actor from account {} started.", self.client);
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        info!("Actor from account {} stopped.", self.client);
    }
}

impl Supervised for AccountHandler {
    fn restarting(&mut self, _: &mut <Self as Actor>::Context) {
        info!("Actor from account {} restarting.", self.client);
    }
}

impl Handler<Transaction> for AccountHandler {
    type Result = Result<(), TransactionError>;

    fn handle(&mut self, tx: Transaction, _ctx: &mut Self::Context) -> Self::Result {
        match tx.transaction_type {
            TransactionType::Deposit => self
                .account
                .deposit(tx.amount.ok_or(TransactionError::InvalidOperation)?, tx.tx),
            TransactionType::Withdrawal => self
                .account
                .withdraw(tx.amount.ok_or(TransactionError::InvalidOperation)?, tx.tx),
            TransactionType::Dispute => self.account.dispute(tx.tx),
            TransactionType::Resolve => self.account.resolve(tx.tx),
            TransactionType::Chargeback => self.account.chargeback(tx.tx),
        }
    }
}

impl Handler<Collect> for AccountHandler {
    type Result = MessageResult<Collect>;

    fn handle(&mut self, _: Collect, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
        MessageResult(self.account.clone())
    }
}
