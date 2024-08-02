use csv::Trim;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;
use std::{collections::HashMap, env, error::Error, ffi::OsString, fs::File, io, process};

mod account;
mod transaction;

use crate::account::*;
use crate::transaction::*;

/// Toy Payments Engine,
/// which processes transactions and stores account states and processed transactions.
/// It stores only fund-moving types of transactions, namely `Deposit` and `Withdraw`,
/// as dispute-related events don't need to be stored.
#[derive(Default)]
struct Engine {
    accounts: HashMap<u32, Account>,
    transactions: HashMap<u32, Transaction>,
}

macro_rules! impl_transaction_handler {
    ($action:ident) => {
        fn $action(&mut self, mut tx: Transaction) -> Result<(), String> {
            tx.execute();
            match tx.state() {
                St::Executed if !self.transactions.contains_key(&tx.id) => {
                    let acc = &mut self.get_or_create_account(tx.client);
                    acc.$action(tx.amount)?;
                }
                r @ _ => return Err(format!("deposit tx declined: {:?}", &r).to_string()),
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
        fn $event(&mut self, tx: &mut Transaction) -> Result<u64, String> {
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
                Tx::Deposit => {
                    tx.$event();
                    match tx.state() {
                        St::$state => acc.$action(tx.amount),
                        r @ _ => return Err(format!("dispute tx declined: {:?}", &r).to_string()),
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

    /// Processes transaction, updating client Account.
    pub fn process(&mut self, mut tx: Transaction) -> Result<(), String> {
        match tx.ty {
            Tx::Deposit => {
                self.deposit(tx)?;
            }
            Tx::Withdrawal => {
                self.withdraw(tx)?;
            }
            Tx::Dispute => {
                self.dispute(&mut tx)?;
            }
            Tx::Resolve => {
                self.resolve(&mut tx)?;
            }
            Tx::Chargeback => {
                self.revert(&mut tx)?;
            }
        };

        Ok(())
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
}

/// TODO Helper for amounts deserialization
fn deserialize_amount_as_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    // First we get u64
    let float = f64::from_str(&buf).map_err(serde::de::Error::custom)?;
    // Then we ensure it's positive
    if float.is_sign_negative() {
        return Err(serde::de::Error::custom(
            "negative tx amounts are not allowed!",
        ));
    }

    let int = (float * 100_000f64).trunc() as u64;

    Ok(int)
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();

    let file_path = get_first_arg()?;
    let file = File::open(file_path)?;
    let mut rdr = csv::ReaderBuilder::new().trim(Trim::All).from_reader(file);
    for entry in rdr.deserialize() {
        let tx: Transaction = entry?;
        engine.process(tx);
    }
    Ok(())
}

/// Returns the first positional argument sent to this process. If there are no
/// positional arguments, then this returns an error.
fn get_first_arg() -> Result<OsString, Box<dyn Error>> {
    match env::args_os().nth(1) {
        None => Err(From::from("expected 1 argument, but got none")),
        Some(file_path) => Ok(file_path),
    }
}

fn main() {
    // let mut wtr = csv::Writer::from_writer(io::stdout());
    // // But now we can write records by providing a normal Rust value.
    // //
    // // Note that the odd `None::<u64>` syntax is required because `None` on
    // // its own doesn't have a concrete type, but Serde needs a concrete type
    // // in order to serialize it. That is, `None` has type `Option<T>` but
    // // `None::<u64>` has type `Option<u64>`.
    // wtr.serialize(Transaction { id: 43, ty: Tx::Deposit, client: 12, amount: 2.345 }).unwrap();
    // wtr.serialize(Transaction { id: 13, ty: Tx::Withdrawal, client: 12, amount: 2.345 }).unwrap();

    if let Err(err) = run() {
        println!("{}", err);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests;
