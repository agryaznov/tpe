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

impl Engine {
    pub fn new() -> Self {
        Default::default()
    }
    /// Processes transaction, updating client Account.
    pub fn process(&mut self, mut tx: Transaction) -> Result<(), String> {
        match tx.ty {
            Tx::Deposit => {
                tx.execute();
                match tx.state() {
                    St::Executed if !self.transactions.contains_key(&tx.id) => {
                        let acc = &mut self.get_or_create_account(tx.client);
                        acc.deposit(tx.amount)?;
                    }
                    r @ _ => return Err(format!("deposit tx declined: {:?}", &r).to_string()),
                }
                // Store succeed transaction
                self.transactions.insert(tx.id, tx);
            }
            Tx::Withdrawal => {
                tx.execute();
                match tx.state() {
                    St::Executed if !self.transactions.contains_key(&tx.id) => {
                        let acc = &mut self.get_or_create_account(tx.client);
                        acc.withdraw(tx.amount)?;
                    }
                    r @ _ => return Err(format!("withdrawal tx declined: {:?}", &r).to_string()),
                }
                // Store succeed transaction
                self.transactions.insert(tx.id, tx);
            }
            Tx::Dispute => {
                self.open_dispute(&mut tx)?;
            }
            Tx::Resolve => {
                self.resolve_dispute(&mut tx)?;
            }
            Tx::Chargeback => {
                self.chargeback(&mut tx)?;
            }
        };

        Ok(())
    }

    /// Opens a dispute on the transaction with provided ID.
    ///
    /// It is impossible to open a dispute if:
    ///
    /// - it refers to a non-existent transaction,
    /// - client ID in the dispute and the disputed transaction don't match.
    ///
    /// Only transactions of the following types can be disputed:
    ///
    /// - Deposit: in this case the deposite amount is held;
    /// - Withdraw: no changes happen on the account balance, we just record
    ///             the fact of the dispute.
    ///
    /// Returns the amount available on the account after the dispute opened.
    fn open_dispute(&mut self, dispute: &mut Transaction) -> Result<u64, String> {
        // lookup for the disputed tx, and fail of not found
        let tx = self
            .transactions
            .get_mut(&dispute.id)
            .ok_or("disputed transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx.client.ne(&dispute.client) {
            return Err("dispute account is not the transaction owner".to_string());
        }
        let acc = &mut self
            .accounts
            .get_mut(&dispute.client)
            .ok_or("dispute account does not exist".to_string())?;

        match tx.ty {
            Tx::Deposit => {
                tx.dispute();
                match tx.state() {
                    St::Disputed => acc.hold(tx.amount),
                    r @ _ => return Err(format!("dispute tx declined: {:?}", &r).to_string()),
                }
            }
            _ => Err("dispute on this type of transaction is not allowed".to_string()),
        }
    }

    /// TODO
    fn resolve_dispute(&mut self, resolve: &Transaction) -> Result<u64, String> {
        // lookup for the disputed tx, and fail of not found
        let tx = self
            .transactions
            .get_mut(&resolve.id)
            .ok_or("disputed transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx.client.ne(&resolve.client) {
            return Err("resolve account is not the transaction owner".to_string());
        }
        let acc = &mut self
            .accounts
            .get_mut(&resolve.client)
            .ok_or("resolve account does not exist".to_string())?;

        match tx.ty {
            Tx::Deposit => {
                tx.resolve();
                match tx.state() {
                    St::Executed => acc.release(tx.amount),
                    r @ _ => return Err(format!("resolve tx declined: {:?}", &r).to_string()),
                }
            }
            _ => Err("dispute/resolve on this type of transaction is not allowed".to_string()),
        }
    }

    /// TODO
    fn chargeback(&mut self, chargeback: &Transaction) -> Result<u64, String> {
        // lookup for the tx to chargeback, and fail of not found
        let tx = self
            .transactions
            .get_mut(&chargeback.id)
            .ok_or("to be charged back transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx.client.ne(&chargeback.client) {
            return Err("chargeback account is not the transaction owner".to_string());
        }
        let acc = &mut self
            .accounts
            .get_mut(&chargeback.client)
            .ok_or("chargeback account does not exist".to_string())?;

        match tx.ty {
            Tx::Deposit => {
                tx.revert();
                match tx.state() {
                    St::Reverted => acc.chargeback(tx.amount),
                    r @ _ => return Err(format!("resolve tx declined: {:?}", &r).to_string()),
                }
            }
            _ => Err(
                "dispute/resolve/chargeback on this type of transaction is not allowed".to_string(),
            ),
        }
    }

    fn get_or_create_account(&mut self, id: u32) -> &mut Account {
        if !&self.accounts.contains_key(&id) {
            self.accounts.insert(id, Account::new(id));
        }
        self.accounts.get_mut(&id).unwrap()
    }
}

/// Helper for amounts deserialization
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
