use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn testx() -> Command {
    Command::cargo_bin("testx").unwrap()
}

#[test]
fn cli_no_args_in_empty_dir() {
    let tmp = TempDir::new().unwrap();
    testx()
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No test framework detected"));
}

#[test]
fn cli_list_subcommand() {
    testx()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Python"))
        .stdout(predicate::str::contains("Rust"))
        .stdout(predicate::str::contains("Go"))
        .stdout(predicate::str::contains("JavaScript"));
}

#[test]
fn cli_detect_rust_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"fake\"").unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust"));
}

#[test]
fn cli_detect_python_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("pytest.ini"), "").unwrap();
    fs::write(tmp.path().join("requirements.txt"), "pytest\n").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Python"));
}

#[test]
fn cli_detect_go_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("go.mod"), "module fake").unwrap();
    fs::write(tmp.path().join("main_test.go"), "package main").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Go"));
}

#[test]
fn cli_detect_js_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("package.json"),
        r#"{"devDependencies":{"vitest":"latest"}}"#,
    )
    .unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("JavaScript"));
}

#[test]
fn cli_detect_empty_dir() {
    let tmp = TempDir::new().unwrap();
    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No test framework detected"));
}

#[test]
fn cli_version_flag() {
    testx()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("testx"));
}

#[test]
fn cli_help_flag() {
    testx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Universal test runner"));
}

// NOTE: Do NOT add tests that run `testx run` against this project.
// That would run `cargo test` which re-runs these integration tests,
// creating an infinite recursive fork bomb that freezes the system.

#[test]
fn cli_path_nonexistent_dir() {
    testx()
        .args(["run", "--path", "/tmp/testx_nonexistent_dir_12345"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("resolve project directory")
                .or(predicate::str::contains("No such file")),
        );
}

#[test]
fn cli_run_empty_dir_error() {
    let tmp = TempDir::new().unwrap();
    testx()
        .args(["run", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No test framework detected"));
}

#[test]
fn cli_detect_polyglot() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
    std::fs::write(tmp.path().join("pyproject.toml"), "[tool.pytest]\n").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust"))
        .stdout(predicate::str::contains("Python"));
}

#[test]
fn cli_list_all_frameworks() {
    let output = testx().arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // All 11 adapters should be listed
    assert!(stdout.contains("Rust"));
    assert!(stdout.contains("Go"));
    assert!(stdout.contains("Python"));
    assert!(stdout.contains("JavaScript") || stdout.contains("JS"));
    assert!(stdout.contains("Java"));
    assert!(stdout.contains("C/C++"));
    assert!(stdout.contains("Ruby"));
    assert!(stdout.contains("Elixir"));
    assert!(stdout.contains("PHP"));
    assert!(stdout.contains("C#/.NET") || stdout.contains("C#"));
    assert!(stdout.contains("Zig"));
}

#[test]
fn cli_detect_java_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("pom.xml"), "<project/>").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Java"));
}

#[test]
fn cli_detect_cpp_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.14)",
    )
    .unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("C/C++"));
}

#[test]
fn cli_detect_ruby_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".rspec"), "--format documentation\n").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ruby"));
}

#[test]
fn cli_detect_elixir_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("mix.exs"), "defmodule MyApp do\nend\n").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Elixir"));
}

#[test]
fn cli_detect_php_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("phpunit.xml"), "<phpunit/>").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("PHP"));
}

#[test]
fn cli_detect_dotnet_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("MyApp.csproj"), "<Project/>").unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("C#"));
}

#[test]
fn cli_detect_zig_project() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("build.zig"),
        "const std = @import(\"std\");",
    )
    .unwrap();

    testx()
        .args(["detect", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Zig"));
}

#[test]
fn cli_init_creates_config() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    testx()
        .args(["init", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    let config = fs::read_to_string(tmp.path().join("testx.toml")).unwrap();
    assert!(config.contains("testx configuration"));
    assert!(config.contains("rust"));
}

#[test]
fn cli_init_refuses_existing() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("testx.toml"), "adapter = \"rust\"").unwrap();

    testx()
        .args(["init", "--path", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}
