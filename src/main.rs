use csv::Trim;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;
use std::{collections::HashMap, env, error::Error, ffi::OsString, fs::File, io, process};

/// User account
#[derive(Default, Debug, Copy, Clone)]
struct Account {
    /// Client ID, unique, one per client.
    id: u32,
    /// Total balance of the client account, including held funds.
    /// We store balances as integers for simpler operations,
    /// as only precision to 10^-5 needed,
    /// we store it as <amount>*10^5.
    total: u64,
    /// Total funds held for dispute.
    held: u64,
    /// Whether the account is locked. An account is locked if a charge back occurs.
    locked: bool,
}

macro_rules! ensure_unlocked {
    ($a:ident) => {
        if $a.locked {
            return Err("account is frozen".to_string());
        }
    };
}

impl Account {
    /// Creates a new client account
    fn new(id: u32) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
    /// Returns available balance of the account.
    pub fn available(&self) -> u64 {
        self.total.saturating_sub(self.held)
    }
    /// Deposits amount to the account.
    /// Returns new total balance upon success.
    pub fn deposit(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.total = self.total.checked_add(amount).ok_or(
            "tx makes balance overflow; such enourmous balances are not supported".to_string(),
        )?;

        Ok(self.total)
    }
    /// Withdraws amount from the account.
    /// Returns new total balance upon success.
    pub fn withdraw(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        if self.available() < amount {
            return Err(format!("insufficient available balance, acc: {:?}", &self).to_string());
        };

        self.total = self
            .total
            .checked_sub(amount)
            .ok_or("insufficient total balance".to_string())?;

        Ok(self.total)
    }
    /// Holds amount on the account.
    /// Returns new available balance upon success.
    pub fn hold(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        // TODO: might be an attack vector?
        self.held = self.held.saturating_add(amount);
        Ok(self.available())
    }
    /// TODO
    pub fn release(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.held = self.held.saturating_sub(amount);
        Ok(self.available())
    }
    /// TODO should we check that this happens AFTER dispute?
    /// Chargeback is possible only if a dispute has been opened for the transaction in question.
    pub fn chargeback(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.total = self.total.saturating_sub(amount);
        self.held = self.held.saturating_sub(amount);

        self.lock();
        Ok(self.available())
    }
    pub fn lock(&mut self) {
        self.locked = true;
    }
    /// TODO
    #[allow(dead_code)]
    pub fn unlock(&mut self) {
        self.locked = false;
    }
}

/// User transaction.
// TODO: fix it to deal woth empty amount field
#[derive(Debug, serde::Deserialize, serde::Serialize, Copy, Clone)]
struct Transaction {
    /// Transaction ID, unique, one per client.
    #[serde(rename = "tx")]
    id: u32,
    /// Transaction type.
    #[serde(rename = "type")]
    ty: Tx,
    /// ID of the client Account performing the Transaction.
    client: u32,
    /// Transacttion amount
    #[serde(default)]
    amount: u64,
    /// Whether the transaction is under dispute.
    #[serde(skip)]
    disputed: bool,
    /// Whether the transaction is charged back
    #[serde(skip)]
    charged_back: bool,
}

#[cfg(test)]
impl Default for Transaction {
    fn default() -> Self {
        Self {
            id: 0,
            ty: Tx::Resolve,
            client: 0,
            amount: 0,
            disputed: false,
            charged_back: false,
        }
    }
}

impl Transaction {
    pub fn dispute(&mut self) {
        self.disputed = true;
    }
    pub fn resolve(&mut self) {
        self.disputed = false;
    }
    pub fn chargeback(&mut self) {
        self.charged_back = true;
    }
}

/// Types of Transactions.
#[derive(Debug, serde::Deserialize, serde::Serialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum Tx {
    /// Credit to client account, increases its available (and therefore total) balance.
    Deposit,
    /// Debit to client account, decreases its available (and therefore total) balance.
    Withdrawal,
    /// Client's claim to reverse an erroneous transaction.
    /// The transaction disputed is the one specified by its ID in the corresponding csv line.
    /// Therefore a dispute does not has its own transaction ID.
    /// This should result in hold of the amount of the corresponding transaction
    /// on the client's account.
    Dispute,
    /// Resolution to a dispute, which is specified by ID of the transaction being disputed.
    Resolve,
    /// Outcome of a dispute which is resolved positively, which is specified by ID of the transaction being disputed.
    Chargeback,
}

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
                let acc = &mut self.get_or_create_account(tx.client);
                acc.deposit(tx.amount)?;
                // Store succeed transaction
                if let Some(t) = self.transactions.insert(tx.id, tx) {
                    return Err(
                        format!("transaction with ID {} already processed", t.id).to_string()
                    );
                }
            }
            Tx::Withdrawal => {
                let acc = &mut self.get_or_create_account(tx.client);
                acc.withdraw(tx.amount)?;
                // If tx succeed, store it
                if let Some(t) = self.transactions.insert(tx.id, tx) {
                    return Err(
                        format!("transaction with ID {} already processed", t.id).to_string()
                    );
                }
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
        let tx_claimed = self
            .transactions
            .get_mut(&dispute.id)
            .ok_or("disputed transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx_claimed.client.ne(&dispute.client) {
            return Err("dispute account is not the transaction owner".to_string());
        }
        let acc = &mut self
            .accounts
            .get_mut(&dispute.client)
            .ok_or("dispute account does not exist".to_string())?;

        match tx_claimed.ty {
            Tx::Deposit => {
                tx_claimed.dispute();
                acc.hold(tx_claimed.amount)
            }
            Tx::Withdrawal => {
                tx_claimed.dispute();
                Ok(0)
            }
            _ => Err("dispute on this type of transaction is not allowed".to_string()),
        }
    }

    /// TODO
    fn resolve_dispute(&mut self, resolve: &Transaction) -> Result<u64, String> {
        // lookup for the disputed tx, and fail of not found
        let tx_claimed = self
            .transactions
            .get_mut(&resolve.id)
            .ok_or("disputed transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx_claimed.client.ne(&resolve.client) {
            return Err("resolve account is not the transaction owner".to_string());
        }
        let acc = &mut self
            .accounts
            .get_mut(&resolve.client)
            .ok_or("resolve account does not exist".to_string())?;

        match tx_claimed.ty {
            Tx::Deposit => {
                tx_claimed.resolve();
                acc.release(tx_claimed.amount)
            }
            Tx::Withdrawal => {
                tx_claimed.resolve();
                Ok(0)
            }
            _ => Err("dispute/resolve on this type of transaction is not allowed".to_string()),
        }
    }

    /// TODO
    fn chargeback(&mut self, chargeback: &Transaction) -> Result<u64, String> {
        // lookup for the tx to chargeback, and fail of not found
        let tx_claimed = self
            .transactions
            .get_mut(&chargeback.id)
            .ok_or("to be charged back transaction not found".to_string())?;
        // ensure accounts match in the dispute claim and in the original transaction
        if tx_claimed.client.ne(&chargeback.client) {
            return Err("chargeback account is not the transaction owner".to_string());
        }
        // only a transaction under a dispute can be charged back
        if !tx_claimed.disputed {
            return Err("to be charged back transaction was not disputed".to_string());
        }

        let acc = &mut self
            .accounts
            .get_mut(&chargeback.client)
            .ok_or("chargeback account does not exist".to_string())?;

        match tx_claimed.ty {
            Tx::Deposit => {
                tx_claimed.chargeback();
                acc.chargeback(tx_claimed.amount)
            }
            Tx::Withdrawal => Ok(0),
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
mod test_utils {
    use super::*;
    use csv::ReaderBuilder;

    pub struct Env {
        engine: Engine,
    }

    impl Env {
        pub fn new() -> Self {
            Env {
                engine: Engine::new(),
            }
        }

        pub fn process(&mut self, data: &str) {
            for t in read_txs(data).unwrap() {
                if let Err(e) = self.process_tx(t) {
                    println!("ERR: {e}")
                }
            }
        }

        pub fn process_tx(&mut self, mut tx: Transaction) -> Result<(), String> {
            self.engine.process(tx)
        }

        pub fn tx_count(&self) -> usize {
            self.engine.transactions.len()
        }

        pub fn acc(&self, id: u32) -> Account {
            *self
                .engine
                .accounts
                .get(&3)
                .clone()
                .expect("account should have been created")
        }

        pub fn balances(&self) -> Vec<(u32, u64)> {
            let mut balances = self
                .engine
                .accounts
                .clone()
                .into_values()
                .map(|v| (v.id, v.total))
                .collect::<Vec<_>>();

            balances.sort_by(|a, b| a.0.cmp(&b.0));
            balances
        }
    }

    pub fn read_txs(csv: &str) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let mut rdr = ReaderBuilder::new()
            .trim(Trim::All)
            .from_reader(csv.as_bytes());
        let mut txs = vec![];
        for entry in rdr.deserialize() {
            let tx: Transaction = entry?;
            txs.push(tx)
        }
        Ok(txs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[test]
    fn deposit_and_withdrawal_work() {
        let mut env = Env::new();

        let data = "\
type, client, tx, amount
deposit, 3, 1, 130042330
deposit, 2, 2, 18000000
withdrawal, 3, 3, 100000000
withdrawal, 2, 4, 5240000
withdrawal, 3, 5, 310000000
";
        // process all transactions
        env.process(data);
        // tx 1,2,3,4 succeed,
        // tx 5 fails with insuficcient balance
        let balances = env.balances();
        let expected = vec![(2, 12_760_000), (3, 30_042_330)];
        assert_eq!(&balances, &expected)
    }

    #[test]
    fn dispute_and_chargeback_work() {
        let mut env = Env::new();
        // TODO: fix it to deal woth empty amount field
        let data = "\
type, client, tx, amount
deposit, 3, 1, 130042330
deposit, 3, 2, 42000000
dispute, 3, 2, 0
chargeback, 3, 1, 0
dispute, 3, 1, 0
chargeback, 3, 2, 0
withdrawal, 3, 3, 100000
deposit, 3, 4, 7000000
";
        // process all transactions
        env.process(data);
        // tx 1: charge fail, then dispute: held
        // tx 2: disputed, charge succeed: withdrew
        // tx 3 & tx 3: fail on forzen account
        assert_eq!(env.tx_count(), 2);

        let acc = env.acc(3);
        assert_eq!(acc.available(), 0);
        assert_eq!(acc.total, 130_042_330);
    }

    #[test]
    fn resolve_works() {
        let mut env = Env::new();

        // TODO: fix it to deal woth empty amount field
        // TODO: add withdrawals
        let data = "\
type, client, tx, amount
deposit, 3, 1, 130042330
deposit, 3, 2, 42000000
dispute, 3, 2, 0
dispute, 3, 1, 0
resolve, 3, 2, 0
";
        // process all transactions
        env.process(data);

        let acc = env.acc(3);
        // ensure one of the disputes resolved
        assert_eq!(acc.available(), 42_000_000);
        assert_eq!(acc.total, 172_042_330);

        // ensure can withdraw resolved
        let data = "\
type, client, tx, amount
resolve, 3, 2, 0
resolve, 3, 1, 0
withdrawal, 3, 3, 172042330
";
        // process next transactions batch
        env.process(data);

        let acc = env.acc(3);
        assert_eq!(acc.total, 0);
    }
}
