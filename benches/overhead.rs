//! Overhead benchmarks: measures testx's detection + parsing overhead.
//!
//! This benchmarks the full "detect → parse" pipeline that runs on top of
//! the underlying test runner, giving a clear picture of testx's added cost.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;
use tempfile::TempDir;
use testx::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};
use testx::detection::DetectionEngine;

// ── Project scaffolding ────────────────────────────────────────────

fn rust_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"bench\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    dir
}

fn python_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]\n").unwrap();
    std::fs::create_dir_all(dir.path().join("tests")).unwrap();
    std::fs::write(dir.path().join("tests/test_a.py"), "def test_a(): pass\n").unwrap();
    dir
}

fn go_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("go.mod"), "module bench\ngo 1.21\n").unwrap();
    std::fs::write(dir.path().join("main_test.go"), "package main\n").unwrap();
    dir
}

fn js_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"bench","scripts":{"test":"vitest run"}}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("vitest.config.ts"), "export default {}\n").unwrap();
    dir
}

fn polyglot_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"poly\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"poly","scripts":{"test":"jest"}}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]\n").unwrap();
    std::fs::write(dir.path().join("go.mod"), "module poly\ngo 1.21\n").unwrap();
    std::fs::write(dir.path().join("main_test.go"), "package main\n").unwrap();
    dir
}

// ── Output generators ──────────────────────────────────────────────

fn rust_output(n: usize) -> String {
    let mut s = format!("\nrunning {n} tests\n");
    for i in 0..n {
        let status = if i % 20 == 0 { "FAILED" } else { "ok" };
        s.push_str(&format!("test module::test_{i} ... {status}\n"));
    }
    let passed = n - n / 20;
    let failed = n / 20;
    s.push_str(&format!(
        "\ntest result: {}. {passed} passed; {failed} failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.67s\n",
        if failed > 0 { "FAILED" } else { "ok" }
    ));
    s
}

fn pytest_output(n: usize) -> String {
    let mut s = String::from(
        "============================= test session starts =============================\ncollected ",
    );
    s.push_str(&n.to_string());
    s.push_str(" items\n\n");
    for i in 0..n {
        let status = if i % 20 == 0 { "FAILED" } else { "PASSED" };
        s.push_str(&format!("tests/test_example.py::test_{i} {status}\n"));
    }
    s.push_str(&format!(
        "\n============================== {n} passed in 3.45s ==============================\n"
    ));
    s
}

fn go_output(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!("=== RUN   TestFunc{i}\n"));
        s.push_str(&format!("--- PASS: TestFunc{i} (0.{:02}s)\n", i % 100));
    }
    s.push_str("PASS\nok  \texample.com/pkg\t2.345s\n");
    s
}

// ── Benchmarks ─────────────────────────────────────────────────────

/// End-to-end: detect best adapter + parse 100-test output.
fn bench_full_pipeline(c: &mut Criterion) {
    let rust_dir = rust_project();
    let py_dir = python_project();
    let go_dir = go_project();
    let js_dir = js_project();

    let rust_out = rust_output(100);
    let py_out = pytest_output(100);
    let go_out = go_output(100);

    let mut group = c.benchmark_group("full_pipeline");

    group.bench_function("rust_detect_and_parse_100", |b| {
        b.iter(|| {
            let engine = DetectionEngine::new();
            let detected = engine.detect(rust_dir.path()).unwrap();
            let adapter = engine.adapter(detected.adapter_index);
            black_box(adapter.parse_output(&rust_out, "", 0));
        });
    });

    group.bench_function("python_detect_and_parse_100", |b| {
        b.iter(|| {
            let engine = DetectionEngine::new();
            let detected = engine.detect(py_dir.path()).unwrap();
            let adapter = engine.adapter(detected.adapter_index);
            black_box(adapter.parse_output(&py_out, "", 0));
        });
    });

    group.bench_function("go_detect_and_parse_100", |b| {
        b.iter(|| {
            let engine = DetectionEngine::new();
            let detected = engine.detect(go_dir.path()).unwrap();
            let adapter = engine.adapter(detected.adapter_index);
            black_box(adapter.parse_output(&go_out, "", 0));
        });
    });

    group.bench_function("js_detect_only", |b| {
        b.iter(|| {
            let engine = DetectionEngine::new();
            black_box(engine.detect(js_dir.path()));
        });
    });

    group.finish();
}

/// Polyglot: detect_all in a project with 4+ markers.
fn bench_polyglot_detection(c: &mut Criterion) {
    let dir = polyglot_project();

    c.bench_function("polyglot_detect_all", |b| {
        b.iter(|| {
            let engine = DetectionEngine::new();
            black_box(engine.detect_all(dir.path()));
        });
    });
}

/// Pure parsing overhead at scale (no I/O).
fn bench_parse_scaling(c: &mut Criterion) {
    let engine = DetectionEngine::new();
    let adapters = engine.adapters();
    let rust_adapter = adapters.iter().find(|a| a.name() == "Rust").unwrap();

    let sizes = [10, 100, 500, 1000, 5000];
    let mut group = c.benchmark_group("parse_scaling_rust");

    for &n in &sizes {
        let output = rust_output(n);
        group.bench_function(format!("{n}_tests"), |b| {
            b.iter(|| black_box(rust_adapter.parse_output(&output, "", 0)));
        });
    }

    group.finish();
}

/// JSON serialization overhead at scale.
fn bench_json_serialization(c: &mut Criterion) {
    let sizes = [10, 100, 1000];
    let mut group = c.benchmark_group("json_serialization");

    for &n in &sizes {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "suite".into(),
                tests: (0..n)
                    .map(|i| TestCase {
                        name: format!("test_{i}"),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(10),
                        error: None,
                    })
                    .collect(),
            }],
            duration: Duration::from_secs(1),
            raw_exit_code: 0,
        };
        group.bench_function(format!("{n}_tests"), |b| {
            b.iter(|| black_box(serde_json::to_string(&result).unwrap()));
        });
    }

    group.finish();
}

/// Config loading overhead.
fn bench_config_loading(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();

    // Without config file
    c.bench_function("config_load_no_file", |b| {
        b.iter(|| {
            black_box(testx::config::Config::load(dir.path()));
        });
    });

    // With config file
    std::fs::write(
        dir.path().join("testx.toml"),
        r#"
args = ["-v", "--no-header"]
timeout = 60

[env]
CI = "true"
RUST_BACKTRACE = "1"

[retry]
count = 3
failed_only = true
"#,
    )
    .unwrap();

    c.bench_function("config_load_with_file", |b| {
        b.iter(|| {
            black_box(testx::config::Config::load(dir.path()));
        });
    });
}

criterion_group!(
    benches,
    bench_full_pipeline,
    bench_polyglot_detection,
    bench_parse_scaling,
    bench_json_serialization,
    bench_config_loading,
);
criterion_main!(benches);
