use criterion::{criterion_group, criterion_main, Criterion};
use toy_payments_engine::Engine;

mod generator;

use generator::generate;

fn basic_benchmark(c: &mut Criterion) {
    // 100k trnasactions
    let fixture = generate(100, 1000).expect("asd");

    let mut engine = Engine::new();

    c.bench_function("run basic", |b| b.iter(|| engine.run(&fixture)));
}

criterion_group!(benches, basic_benchmark);
criterion_main!(benches);
