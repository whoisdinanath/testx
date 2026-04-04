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

// ─── Workspace CLI tests ───

#[test]
fn cli_workspace_list_discovers_projects() {
    let tmp = TempDir::new().unwrap();

    // Create a Rust project
    let rust_dir = tmp.path().join("svc-rust");
    fs::create_dir_all(&rust_dir).unwrap();
    fs::write(rust_dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

    // Create a Go project
    let go_dir = tmp.path().join("svc-go");
    fs::create_dir_all(&go_dir).unwrap();
    fs::write(go_dir.join("go.mod"), "module example.com/svc\n").unwrap();
    fs::write(go_dir.join("main_test.go"), "package main\n").unwrap();

    testx()
        .args([
            "workspace",
            "--list",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Discovered"))
        .stdout(predicate::str::contains("Rust"))
        .stdout(predicate::str::contains("Go"));
}

#[test]
fn cli_workspace_list_empty() {
    let tmp = TempDir::new().unwrap();

    testx()
        .args([
            "workspace",
            "--list",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No testable projects found"));
}

#[test]
fn cli_workspace_filter_language() {
    let tmp = TempDir::new().unwrap();

    // Create Rust + Go projects
    let rust_dir = tmp.path().join("svc-rust");
    fs::create_dir_all(&rust_dir).unwrap();
    fs::write(rust_dir.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();

    let go_dir = tmp.path().join("svc-go");
    fs::create_dir_all(&go_dir).unwrap();
    fs::write(go_dir.join("go.mod"), "module example.com/s\n").unwrap();
    fs::write(go_dir.join("main_test.go"), "package main\n").unwrap();

    testx()
        .args([
            "workspace",
            "--list",
            "--filter",
            "rust",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust"))
        .stdout(predicate::str::contains("1 project"));
}

#[test]
fn cli_workspace_max_depth() {
    let tmp = TempDir::new().unwrap();

    // Create deeply nested project
    let deep = tmp.path().join("a").join("b").join("c").join("d").join("e");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("Cargo.toml"), "[package]\nname = \"deep\"\n").unwrap();

    testx()
        .args([
            "workspace",
            "--list",
            "--max-depth",
            "2",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No testable projects found"));
}

#[test]
fn cli_workspace_run_rust_project() {
    let tmp = TempDir::new().unwrap();

    // Create a minimal Rust project that compiles
    let rust_dir = tmp.path().join("my-crate");
    fs::create_dir_all(rust_dir.join("src")).unwrap();
    fs::write(
        rust_dir.join("Cargo.toml"),
        "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        rust_dir.join("src/lib.rs"),
        "#[test] fn it_works() { assert_eq!(2 + 2, 4); }\n",
    )
    .unwrap();

    testx()
        .args(["workspace", "--path", tmp.path().to_str().unwrap()])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("1 passed"));
}

#[test]
fn cli_workspace_json_output() {
    let tmp = TempDir::new().unwrap();

    let rust_dir = tmp.path().join("my-crate");
    fs::create_dir_all(rust_dir.join("src")).unwrap();
    fs::write(
        rust_dir.join("Cargo.toml"),
        "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        rust_dir.join("src/lib.rs"),
        "#[test] fn it_works() { assert_eq!(1, 1); }\n",
    )
    .unwrap();

    testx()
        .args([
            "workspace",
            "-o",
            "json",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"projects_found\""))
        .stdout(predicate::str::contains("\"total_tests\""));
}

#[test]
fn cli_workspace_sequential() {
    let tmp = TempDir::new().unwrap();

    let rust_dir = tmp.path().join("my-crate");
    fs::create_dir_all(rust_dir.join("src")).unwrap();
    fs::write(
        rust_dir.join("Cargo.toml"),
        "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        rust_dir.join("src/lib.rs"),
        "#[test] fn ok() { assert!(true); }\n",
    )
    .unwrap();

    testx()
        .args([
            "workspace",
            "--sequential",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("sequential"))
        .stdout(predicate::str::contains("1 passed"));
}

// ─── Stress CLI tests ───

#[test]
fn cli_stress_basic() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"stress-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        "#[test] fn stable() { assert!(true); }\n",
    )
    .unwrap();

    testx()
        .args(["stress", "-n", "2", "--path", tmp.path().to_str().unwrap()])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("Stress Test Report"))
        .stdout(predicate::str::contains("2/2 iterations"))
        .stdout(predicate::str::contains("no flaky tests"));
}

#[test]
fn cli_stress_json_output() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"stress-json\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        "#[test] fn ok() { assert!(true); }\n",
    )
    .unwrap();

    testx()
        .args([
            "stress",
            "-n",
            "2",
            "-o",
            "json",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("iterations_completed"))
        .stdout(predicate::str::contains("all_passed"));
}

#[test]
fn cli_stress_with_threshold() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"stress-thr\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        "#[test] fn ok() { assert!(true); }\n",
    )
    .unwrap();

    // All tests pass so threshold should pass
    testx()
        .args([
            "stress",
            "-n",
            "2",
            "--threshold",
            "0.9",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .success()
        .stdout(predicate::str::contains("no flaky tests"));
}

#[test]
fn cli_stress_fail_fast() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"stress-ff\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    // This test always fails, so fail-fast should stop after iteration 1
    fs::write(
        tmp.path().join("src/lib.rs"),
        "#[test] fn always_fail() { panic!(\"always\"); }\n",
    )
    .unwrap();

    testx()
        .args([
            "stress",
            "-n",
            "5",
            "--fail-fast",
            "--path",
            tmp.path().to_str().unwrap(),
        ])
        .timeout(std::time::Duration::from_secs(120))
        .assert()
        .failure()
        .stdout(predicate::str::contains("1/5 iterations"))
        .stdout(predicate::str::contains("stopped early"));
}
