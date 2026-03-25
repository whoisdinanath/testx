//! Benchmarks for framework detection.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;

fn create_rust_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "bench-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    dir
}

fn create_node_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "bench", "version": "1.0.0", "scripts": {"test": "jest"}}"#,
    )
    .unwrap();
    dir
}

fn create_python_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("setup.py"), "").unwrap();
    std::fs::create_dir_all(dir.path().join("tests")).unwrap();
    std::fs::write(dir.path().join("tests/test_example.py"), "def test_(): pass").unwrap();
    dir
}

fn create_go_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("go.mod"), "module bench\ngo 1.21\n").unwrap();
    std::fs::write(dir.path().join("main_test.go"), "package main").unwrap();
    dir
}

fn create_empty_project() -> TempDir {
    TempDir::new().unwrap()
}

fn bench_detect_rust(c: &mut Criterion) {
    let dir = create_rust_project();
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_rust", |b| {
        b.iter(|| {
            black_box(engine.detect_all(dir.path()));
        });
    });
}

fn bench_detect_node(c: &mut Criterion) {
    let dir = create_node_project();
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_node", |b| {
        b.iter(|| {
            black_box(engine.detect_all(dir.path()));
        });
    });
}

fn bench_detect_python(c: &mut Criterion) {
    let dir = create_python_project();
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_python", |b| {
        b.iter(|| {
            black_box(engine.detect_all(dir.path()));
        });
    });
}

fn bench_detect_go(c: &mut Criterion) {
    let dir = create_go_project();
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_go", |b| {
        b.iter(|| {
            black_box(engine.detect_all(dir.path()));
        });
    });
}

fn bench_detect_empty(c: &mut Criterion) {
    let dir = create_empty_project();
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_empty", |b| {
        b.iter(|| {
            black_box(engine.detect_all(dir.path()));
        });
    });
}

fn bench_detect_all_types(c: &mut Criterion) {
    let dirs: Vec<(String, TempDir)> = vec![
        ("rust".into(), create_rust_project()),
        ("node".into(), create_node_project()),
        ("python".into(), create_python_project()),
        ("go".into(), create_go_project()),
        ("empty".into(), create_empty_project()),
    ];
    let engine = testx::detection::DetectionEngine::new();

    c.bench_function("detect_all_types", |b| {
        b.iter(|| {
            for (_, dir) in &dirs {
                black_box(engine.detect_all(dir.path()));
            }
        });
    });
}

criterion_group!(
    benches,
    bench_detect_rust,
    bench_detect_node,
    bench_detect_python,
    bench_detect_go,
    bench_detect_empty,
    bench_detect_all_types,
);
criterion_main!(benches);
