use criterion::{criterion_group, criterion_main, Criterion};
use std::io;
use toy_payments_engine::Engine;

mod generator;

use generator::generate;

fn one_mil_benchmark(c: &mut Criterion) {
    // 1M transactions: 1000x4x250
    const CLIENTS: usize = 1000;
    const TRANSACTIONS: usize = 249;
    let fixture = generate(CLIENTS, TRANSACTIONS).expect("can't find/generate corpus file");

    let mut engine = Engine::new();

    c.bench_function("1M transactions", |b| {
        b.iter(|| engine.run(&fixture, io::empty()))
    });

    assert_eq!(engine.accounts().len(), CLIENTS);
    assert_eq!(
        engine.transactions().len(),
        (2 * TRANSACTIONS + 1) * CLIENTS
    );
    for (balance, locked, acc) in engine.accounts().map(|a| (a.total, a.locked, a.id)) {
        assert_eq!((balance, locked), (0, true), "account: {acc}")
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = one_mil_benchmark
}

criterion_main!(benches);
