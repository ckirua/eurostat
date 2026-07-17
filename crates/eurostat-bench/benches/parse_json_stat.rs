use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eurostat::parse::json_stat::parse_json_stat;

fn bench_json_stat(c: &mut Criterion) {
    let data = include_bytes!("../../eurostat/tests/fixtures/nama_10_gdp.json");
    c.bench_function("parse_json_stat", |b| {
        b.iter(|| parse_json_stat(black_box(data), "nama_10_gdp").expect("parse"));
    });
}

criterion_group!(benches, bench_json_stat);
criterion_main!(benches);
