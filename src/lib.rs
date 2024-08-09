use csv::Trim;
use std::collections::hash_map::{HashMap, Values};
use std::{error::Error, ffi::OsString, fs::File, io};

mod account;
mod transaction;

pub use account::*;
pub use transaction::*;

/// Toy Payments Engine,
/// which processes transactions and stores account states and processed transactions.
/// It stores only fund-moving types of transactions, namely `Deposit` and `Withdraw`,
/// as dispute-related events don't need to be stored.
#[derive(Debug, Default)]
pub struct Engine {
    accounts: HashMap<u32, Account>,
    transactions: HashMap<u32, Transaction>,
}

macro_rules! impl_transaction_handler {
    ($action:ident) => {
        fn $action(&mut self, mut tx: Transaction) -> Result<(), String> {
            tx.execute();
            match tx.state() {
                State::Executed if !self.transactions.contains_key(&tx.id) => {
                    let acc = &mut self.get_or_create_account(tx.client);
                    acc.$action(tx.amount.ok_or("empty amount")?)?;
                }
                r => return Err(format!("deposit/withdrawal tx declined: {:?}", &r)),
            }
            // Store succeed transaction
            self.transactions.insert(tx.id, tx);
            Ok(())
        }
    };
}

macro_rules! impl_event_handler {
    ($event:ident, $action:ident, $state:ident) => {
        #[doc = "Handles "]
        #[doc = stringify!($event)]
        #[doc = " request by performing safety checks, and performing `"]
        #[doc = stringify!($action)]
        #[doc = "()` action on the account balance. Succeed only if the transaction in question "]
        #[doc = "ended up at the `"]
        #[doc = stringify!($state)]
        #[doc = "` state."]
        fn $event(&mut self, tx: &mut Transaction) -> Result<(), String> {
            // lookup for the disputed tx, and fail if not found
            let tx = &mut self
                .transactions
                .get_mut(&tx.id)
                .ok_or("disputed transaction not found".to_string())?;
            // ensure accounts match in the dispute claim and in the original transaction,
            // this is kinda authentication.
            if tx.client.ne(&tx.client) {
                return Err("dispute account is not the transaction owner".to_string());
            }
            let acc = &mut self
                .accounts
                .get_mut(&tx.client)
                .ok_or("dispute account does not exist".to_string())?;

            match tx.ty {
                // only deposit transactions can be disputed
                Some(Tx::Deposit) => {
                    tx.$event();
                    match tx.state() {
                        State::$state => acc.$action(tx.amount.ok_or("empty amount")?).map(|_| ()),

                        r => Err(format!("dispute tx declined: {:?}", &r)),
                    }
                }
                _ => Err("dispute on this type of transaction is not allowed".to_string()),
            }
        }
    };
}

impl Engine {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn run(&mut self, file_path: &OsString) -> Result<(), Box<dyn Error>> {
        let file = File::open(file_path)?;
        let mut rdr = csv::ReaderBuilder::new()
            .trim(Trim::All)
            .flexible(true)
            .from_reader(file);
        // input
        // ignores failed to be parsed entries
        for entry in rdr.deserialize().flatten() {
            // load
            let mut tx: Transaction = entry;
            let s = Box::new(Received);
            if tx.init(s).is_ok() {
                // process
                // infalible run, we ignore errors,
                // faulty transactions are simply discarded
                let _ = self.process(tx);
            }
        }
        // output
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(true)
            .from_writer(io::stdout());
        for client in self.accounts() {
            wtr.serialize(AccountSer::from(*client))?
        }
        wtr.flush()?;

        Ok(())
    }

    /// Processes transaction, updating client Account.
    pub fn process(&mut self, mut tx: Transaction) -> Result<(), String> {
        match tx.ty {
            Some(Tx::Deposit) => self.deposit(tx),
            Some(Tx::Withdrawal) => self.withdraw(tx),
            Some(Tx::Dispute) => self.dispute(&mut tx),
            Some(Tx::Resolve) => self.resolve(&mut tx),
            Some(Tx::Chargeback) => self.revert(&mut tx),
            None => Err("transaction type not specified".to_string()),
        }
    }

    impl_transaction_handler!(deposit);
    impl_transaction_handler!(withdraw);
    impl_event_handler!(dispute, hold, Disputed);
    impl_event_handler!(resolve, release, Executed);
    impl_event_handler!(revert, chargeback, Reverted);

    fn get_or_create_account(&mut self, id: u32) -> &mut Account {
        if !&self.accounts.contains_key(&id) {
            self.accounts.insert(id, Account::new(id));
        }
        self.accounts.get_mut(&id).unwrap()
    }

    pub fn accounts(&self) -> Values<u32, Account> {
        self.accounts.values()
    }

    pub fn transactions(&self) -> Values<u32, Transaction> {
        self.transactions.values()
    }

    pub fn get_account(&self, id: &u32) -> Option<&Account> {
        self.accounts.get(id)
    }
}
