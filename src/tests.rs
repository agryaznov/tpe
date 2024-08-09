use csv::Trim;
use test_utils::*;
use toy_payments_engine::*;

#[test]
fn amount_deserializing_works() {
    let mut env = Env::new();
    let data = format!(
        "\
type, client, tx, amount
deposit, 1, 1, .12345
deposit, 2, 2, 0.12345
deposit, 3, 3, 1.23456
deposit, 4, 4, 1.
# this is maximum allowed
deposit, 5, 10, {0}.{1},
# well this is more but treated the same at 10^-4 precision
deposit, 6, 11, {0}.{1}99999,
# and we can't deposit more to these accounts
# [2 fails]
deposit, 5, 12, 1
deposit, 6, 11, 1
# this is miminum possible
deposit, 7, 7, 0.0001
# those should fail on parsing
# [5 fails]
deposit, 4, 5, -1
deposit, 4, 6, 10e20
deposit, 4, 7,
deposit, 4, 8
deposit, 4, 9, {2}.{3}
# this parsed to 0 and declined on tx execution
# [1 fail]
deposit, 7, 7, 0.00009
",
        u64::MAX / 10_000,
        u64::MAX % 10_000,
        u64::MAX / 10_000,
        u64::MAX % 10_000 + 1,
    );
    // process all transactions
    env.process(&data);
    println!("env: {:#?}", &env);

    // shuold have 15 [total] - 8 [fail] = 7 transactions stored
    assert_eq!(env.tx_count(), 7);
    // shuold result in following balances
    assert_eq!(env.acc(1).total, 1234);
    assert_eq!(env.acc(2).total, 1234);
    assert_eq!(env.acc(3).total, 12345);
    assert_eq!(env.acc(4).total, 10000);
    assert_eq!(env.acc(5).total, u64::MAX);
    assert_eq!(env.acc(6).total, u64::MAX);
    assert_eq!(env.acc(7).total, 1);
}

#[test]
fn deposit_and_withdrawal_work() {
    let mut env = Env::new();
    let data = "\
type, client, tx, amount
# succeed
deposit, 3, 1, 1300.42339
# succeed
deposit, 2, 2, 180
# succeed
deposit, 2, 3, 220.0003
# refused: already processed
deposit, 2, 3, 220.0003
# succeed
withdrawal, 3, 4, 1000.0001
# refused: already processed
withdrawal, 3, 4, 1000.0001
# refused: insuficcient balance
withdrawal, 2, 5, 400.0004
# succeed
withdrawal, 2, 6, 400.000299
# these 2 should result in +2 (not +3!)
deposit, 1, 7, 0.00019
deposit, 1, 8, 0.00019
";
    // process all transactions
    env.process(data);
    println!("env: {:#?}", &env);
    // tx 1,2,3,4 succeed,
    // tx 5 fails with
    // tx 6 succeed
    let balances = env.balances();
    let expected = vec![(1, 2), (2, 1), (3, 3004232)];
    assert_eq!(&balances, &expected)
}

#[test]
fn dispute_and_resolve_work() {
    let mut env = Env::new();

    let data = "\
type, client, tx, amount
deposit, 3, 1, 10000
deposit, 3, 2, 42000
dispute, 3, 2,
dispute, 3, 1
withdrawal, 3, 3, 12000
resolve, 3, 2, 0
withdrawal, 3, 3, 12000
";
    // process all transactions
    env.process(data);
    println!("env: {:#?}", &env);
    let acc = env.acc(3);
    // ensure one of the disputes resolved
    assert_eq!(acc.available(), 300_000_000);
    assert_eq!(acc.total, 400_000_000);
}

#[test]
fn chargeback_works() {
    let mut env = Env::new();
    let data = "\
type, client, tx, amount
deposit, 3, 1, 1300.4233
deposit, 3, 2, 420
chargeback, 3, 1, 0
dispute, 3, 1, 100
chargeback, 3, 1, 0
withdrawal, 3, 3, 100000
deposit, 3, 4, 70
";
    // process all transactions
    env.process(data);
    println!("env: {:#?}", &env);
    // 1,2: ok
    // 3: cb fail
    // 4,5: ok
    // 5,6: fail on frozen account
    assert_eq!(env.tx_count(), 2);

    let acc = env.acc(3);
    assert_eq!(acc.available(), 4200000);
    assert_eq!(acc.total, 4200000);
}

#[test]
fn ignores_faulty_records() {
    let mut env = Env::new();

    let data = "\
type, client, tx, amount
deposit, 1, 1, 10000
d p s t, 1, 2, 1000
deposit, 1, 3, -100
deposit, 1, -3, -10
withdraw, 1, 4, 0
deposit, 1, 4, 0
withdraw, 1, 5
dispute, a, b, c, d, f
,,,,
,..abrakadabra!
";
    // process all transactions
    env.process(data);
    println!("env: {:#?}", &env);
    let acc = env.acc(1);
    // ensure only one tx succeed (#1)
    assert_eq!(env.tx_count(), 1);
    assert_eq!(acc.total, 100_000_000);
}

#[cfg(test)]
mod test_utils {
    use super::*;
    use csv::ReaderBuilder;

    #[derive(Debug)]
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
            for t in read_txs(data) {
                if let Err(e) = self.process_tx(t) {
                    println!("ERR: {e}")
                }
            }
        }

        pub fn process_tx(&mut self, tx: Transaction) -> Result<(), String> {
            self.engine.process(tx)
        }

        pub fn tx_count(&self) -> usize {
            self.engine.transactions().len()
        }

        pub fn acc(&self, id: u32) -> Account {
            *self
                .engine
                .get_account(&id)
                .expect("account should have been created")
        }

        pub fn balances(&self) -> Vec<(u32, u64)> {
            let mut balances = self
                .engine
                .accounts()
                .map(|v| (v.id, v.total))
                .collect::<Vec<_>>();

            balances.sort_by(|a, b| a.0.cmp(&b.0));
            balances
        }
    }

    pub fn read_txs(csv: &str) -> Vec<Transaction> {
        let mut rdr = ReaderBuilder::new()
            .trim(Trim::All)
            .flexible(true)
            .from_reader(csv.as_bytes());
        let mut txs = vec![];
        for entry in rdr.deserialize().flatten() {
            let mut tx: Transaction = entry;
            let s = Box::new(Received);
            if tx.init(s).is_ok() {
                txs.push(tx)
            }
        }
        txs
    }
}
