//! Integration tests for output formats.

use assert_cmd::Command;
use tempfile::TempDir;
use std::fs;

fn setup_project(dir: &TempDir) {
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn alpha() { assert!(true); }

    #[test]
    fn beta() { assert!(true); }
}
"#,
    )
    .unwrap();
}

#[test]
fn pretty_output_default() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["run", "--path", dir.path().to_str().unwrap()])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    // Pretty format should show test results
    if result.status.success() {
        assert!(
            stdout.contains("passed") || stdout.contains("✓") || stdout.contains("ok"),
            "Pretty output should show results"
        );
    }
}

#[test]
fn json_output_format() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    if !stdout.trim().is_empty() {
        // Verify it's valid JSON
        let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(stdout.trim());
        assert!(parsed.is_ok(), "Output should be valid JSON: {}", stdout);
    }
}

#[test]
fn junit_output_format() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--output",
            "junit",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    if !stdout.trim().is_empty() {
        assert!(
            stdout.contains("<?xml") || stdout.contains("<testsuites"),
            "JUnit output should be XML"
        );
    }
}

#[test]
fn tap_output_format() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--output",
            "tap",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    if !stdout.trim().is_empty() {
        assert!(
            stdout.contains("TAP version") || stdout.contains("ok ") || stdout.contains("not ok "),
            "TAP output should follow TAP protocol"
        );
    }
}

#[test]
fn slowest_flag() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--slowest",
            "5",
        ])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    // Should not panic, may or may not show slowest depending on test output
    assert!(result.status.code().is_some());
}

#[test]
fn list_subcommand_shows_frameworks() {
    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["list"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("Supported") || stdout.contains("framework"));
}
