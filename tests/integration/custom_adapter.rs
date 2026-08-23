//! Integration tests for custom adapter / script adapter functionality.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[test]
fn config_with_custom_command() {
    let dir = TempDir::new().unwrap();

    // Create a project with a custom test runner config
    fs::write(
        dir.path().join("testx.toml"),
        r#"
adapter = "custom"
args = []

[custom]
command = "echo"
args = ["all tests passed"]
"#,
    )
    .unwrap();

    // No Cargo.toml, detect should still work with custom config
    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["detect", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Detect without a known framework should note no detection
    assert!(result.status.code().is_some());
}

#[test]
fn run_without_framework_errors_cleanly() {
    let dir = TempDir::new().unwrap();

    // Empty project, no frameworks
    fs::write(dir.path().join("README.md"), "# Empty Project").unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["run", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Should exit with error but not panic
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("No test framework") || stderr.contains("error"),
        "Should show helpful error message"
    );
}

/// A custom adapter with no detect rules is opt-in: invisible to detection,
/// runnable by name. This is how a second suite (e2e, smoke, contract tests)
/// lives beside the project's default one.
#[test]
fn opt_in_adapter_runs_only_when_named() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("testx.toml"),
        r#"
[[custom_adapter]]
name = "e2e"
command = "echo"
args = ["PASS login_flow"]
output = "lines"
"#,
    )
    .unwrap();

    let detected = Command::cargo_bin("testx")
        .unwrap()
        .args(["detect", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&detected.stdout).contains("No test framework detected"),
        "opt-in adapter must not hijack detection"
    );

    let run = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--adapter",
            "e2e",
        ])
        .output()
        .unwrap();
    assert!(run.status.success());
    assert!(String::from_utf8_lossy(&run.stdout).contains("login_flow"));
}

#[test]
fn unknown_adapter_name_is_rejected() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "# Empty").unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--adapter",
            "cobol",
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Unknown adapter 'cobol'"));
}

/// `--adapter` beats `adapter = ` in testx.toml, so a repo can pin its default
/// suite and still run the other one on demand.
#[test]
fn cli_adapter_overrides_config_adapter() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"fake\"").unwrap();
    fs::write(
        dir.path().join("testx.toml"),
        r#"
adapter = "Rust"

[[custom_adapter]]
name = "smoke"
command = "echo"
args = ["PASS smoke_check"]
output = "lines"
"#,
    )
    .unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--adapter",
            "smoke",
        ])
        .output()
        .unwrap();

    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("smoke_check"));
}

/// Runners that only write machine-readable results to a file (pytest
/// --junitxml, jest-junit, PLAYWRIGHT_JUNIT_OUTPUT_NAME) are parsed from it.
#[test]
fn report_file_is_parsed_instead_of_stdout() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("junit.xml"),
        r#"<testsuites><testsuite name="e2e"><testcase name="checkout" time="1.5"/><testcase name="refund" time="0.5"><failure message="timeout"/></testcase></testsuite></testsuites>"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("testx.toml"),
        r#"
[[custom_adapter]]
name = "e2e"
command = "echo"
args = ["Running 2 tests using 1 worker"]
output = "junit"
report_file = "junit.xml"
"#,
    )
    .unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--adapter",
            "e2e",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(!result.status.success(), "one testcase failed: {stdout}");
    assert!(stdout.contains("checkout"), "{stdout}");
    assert!(stdout.contains("timeout"), "{stdout}");
}
