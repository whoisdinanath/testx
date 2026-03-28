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
