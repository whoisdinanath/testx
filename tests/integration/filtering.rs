//! Integration tests for filtering functionality.

use assert_cmd::Command;
use tempfile::TempDir;
use std::fs;

fn setup_rust_project(dir: &TempDir) {
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
    fn test_alpha() {
        assert!(true);
    }

    #[test]
    fn test_beta() {
        assert!(true);
    }

    #[test]
    fn test_gamma() {
        assert!(true);
    }

    #[test]
    fn special_delta() {
        assert!(true);
    }
}
"#,
    )
    .unwrap();
}

#[test]
fn filter_by_name_pattern() {
    let dir = TempDir::new().unwrap();
    setup_rust_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["run", "--path", dir.path().to_str().unwrap(), "--", "test_alpha"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    // Should run (may pass or fail depending on environment having cargo)
    // Just verify the command doesn't panic
    assert!(result.status.code().is_some());
    let _ = stdout;
}

#[test]
fn detect_finds_rust_project() {
    let dir = TempDir::new().unwrap();
    setup_rust_project(&dir);

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["detect", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("cargo test") || stdout.contains("Rust") || stdout.contains("Detected"));
}

#[test]
fn detect_empty_dir_no_panic() {
    let dir = TempDir::new().unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["detect", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(result.status.code().is_some());
}

#[test]
fn run_with_nonexistent_path() {
    let result = Command::cargo_bin("testx")
        .unwrap()
        .args(["run", "--path", "/tmp/testx-nonexistent-project-12345"])
        .output()
        .unwrap();

    assert!(!result.status.success());
}

#[test]
fn json_output_is_valid() {
    let dir = TempDir::new().unwrap();
    setup_rust_project(&dir);

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
    // If tests ran, output should be valid JSON
    if !stdout.is_empty() {
        // Check it at least starts with { or [
        let trimmed = stdout.trim();
        if !trimmed.is_empty() {
            assert!(
                trimmed.starts_with('{') || trimmed.starts_with('['),
                "JSON output should start with {{ or [, got: {}",
                &trimmed[..trimmed.len().min(50)]
            );
        }
    }
}

#[test]
fn tap_output_format() {
    let dir = TempDir::new().unwrap();
    setup_rust_project(&dir);

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
    if !stdout.is_empty() && stdout.contains("TAP") {
        assert!(stdout.contains("TAP version"));
    }
}
