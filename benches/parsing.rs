//! Benchmarks for test output parsing.

use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn generate_rust_output(n: usize) -> String {
    let mut output = String::new();
    output.push_str("\nrunning ");
    output.push_str(&n.to_string());
    output.push_str(" tests\n");

    for i in 0..n {
        output.push_str(&format!("test test_{i} ... ok\n"));
    }

    output.push_str(&format!(
        "\ntest result: ok. {n} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s\n"
    ));
    output
}

fn generate_go_output(n: usize) -> String {
    let mut output = String::new();
    for i in 0..n {
        output.push_str(&format!(
            "--- PASS: TestFunc{i} (0.{:02}s)\n",
            i % 100
        ));
    }
    output.push_str("PASS\nok  \tpkg\t1.234s\n");
    output
}

fn generate_pytest_output(n: usize) -> String {
    let mut output = String::new();
    output.push_str("============================= test session starts =============================\n");
    output.push_str("collected ");
    output.push_str(&n.to_string());
    output.push_str(" items\n\n");

    for i in 0..n {
        output.push_str(&format!("tests/test_example.py::test_{i} PASSED\n"));
    }

    output.push_str(&format!(
        "\n============================== {n} passed in 1.23s ==============================\n"
    ));
    output
}

fn bench_parse_rust(c: &mut Criterion) {
    let small = generate_rust_output(10);
    let medium = generate_rust_output(100);
    let large = generate_rust_output(1000);

    let engine = testx::detection::DetectionEngine::new();
    let adapters = engine.adapters();
    let rust_adapter = adapters.iter().find(|a| a.name() == "cargo test").unwrap();

    let mut group = c.benchmark_group("parse_rust");
    group.bench_function("10_tests", |b| {
        b.iter(|| black_box(rust_adapter.parse_output(&small, "", 0)));
    });
    group.bench_function("100_tests", |b| {
        b.iter(|| black_box(rust_adapter.parse_output(&medium, "", 0)));
    });
    group.bench_function("1000_tests", |b| {
        b.iter(|| black_box(rust_adapter.parse_output(&large, "", 0)));
    });
    group.finish();
}

fn bench_parse_go(c: &mut Criterion) {
    let small = generate_go_output(10);
    let medium = generate_go_output(100);
    let large = generate_go_output(1000);

    let engine = testx::detection::DetectionEngine::new();
    let adapters = engine.adapters();
    let go_adapter = adapters.iter().find(|a| a.name() == "go test").unwrap();

    let mut group = c.benchmark_group("parse_go");
    group.bench_function("10_tests", |b| {
        b.iter(|| black_box(go_adapter.parse_output(&small, "", 0)));
    });
    group.bench_function("100_tests", |b| {
        b.iter(|| black_box(go_adapter.parse_output(&medium, "", 0)));
    });
    group.bench_function("1000_tests", |b| {
        b.iter(|| black_box(go_adapter.parse_output(&large, "", 0)));
    });
    group.finish();
}

fn bench_parse_pytest(c: &mut Criterion) {
    let small = generate_pytest_output(10);
    let medium = generate_pytest_output(100);
    let large = generate_pytest_output(1000);

    let engine = testx::detection::DetectionEngine::new();
    let adapters = engine.adapters();
    let py_adapter = adapters.iter().find(|a| a.name() == "pytest").unwrap();

    let mut group = c.benchmark_group("parse_pytest");
    group.bench_function("10_tests", |b| {
        b.iter(|| black_box(py_adapter.parse_output(&small, "", 0)));
    });
    group.bench_function("100_tests", |b| {
        b.iter(|| black_box(py_adapter.parse_output(&medium, "", 0)));
    });
    group.bench_function("1000_tests", |b| {
        b.iter(|| black_box(py_adapter.parse_output(&large, "", 0)));
    });
    group.finish();
}

criterion_group!(benches, bench_parse_rust, bench_parse_go, bench_parse_pytest);
criterion_main!(benches);
