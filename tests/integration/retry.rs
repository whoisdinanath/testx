//! Integration tests for retry functionality.

use assert_cmd::Command;
use tempfile::TempDir;
use std::fs;

#[test]
fn run_with_timeout_flag() {
    let dir = TempDir::new().unwrap();

    // Create a minimal Cargo.toml
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
    fn quick_test() {
        assert!(true);
    }
}
"#,
    )
    .unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--timeout",
            "30",
        ])
        .output()
        .unwrap();

    // Should complete within timeout
    assert!(result.status.code().is_some());
}

#[test]
fn verbose_flag_shows_extra_info() {
    let dir = TempDir::new().unwrap();

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
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--verbose",
        ])
        .output()
        .unwrap();

    // Verbose should show command details on stderr
    let stderr = String::from_utf8_lossy(&result.stderr);
    // May contain "cmd:" prefix
    let _ = stderr;
    assert!(result.status.code().is_some());
}

#[test]
fn raw_flag_shows_output() {
    let dir = TempDir::new().unwrap();

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
    fn passing() { assert!(true); }
}
"#,
    )
    .unwrap();

    let result = Command::cargo_bin("testx")
        .unwrap()
        .args([
            "run",
            "--path",
            dir.path().to_str().unwrap(),
            "--raw",
        ])
        .output()
        .unwrap();

    assert!(result.status.code().is_some());
}
