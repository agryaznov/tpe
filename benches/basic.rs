use std::ffi::OsString;

use criterion::{criterion_group, criterion_main, Criterion};

use toy_payments_engine::Engine;

fn basic_benchmark(c: &mut Criterion) {
    let mut engine = Engine::new();
    let mut fixture = OsString::new();
    fixture.push("../fixtures/basic.csv");

    c.bench_function("run basic", |b| b.iter(|| engine.run(&fixture)));
}

criterion_group!(benches, basic_benchmark);
criterion_main!(benches);
