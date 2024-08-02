use super::*;
use test_utils::*;

#[test]
fn deposit_and_withdrawal_work() {
    let mut env = Env::new();

    let data = "\
type, client, tx, amount
deposit, 3, 1, 130042330
deposit, 2, 2, 18000000
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
    // tx 3 & tx 4: fail on frozen account
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
            let mut tx: Transaction = entry?;
            let s = Box::new(Received);
            tx.init(s);
            println!("loaded tx: {:?}", &tx);
            txs.push(tx)
        }
        Ok(txs)
    }
}
