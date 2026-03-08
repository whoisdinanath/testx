use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct ZigAdapter;

impl Default for ZigAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZigAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl TestAdapter for ZigAdapter {
    fn name(&self) -> &str {
        "Zig"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("zig").is_err() {
            return Some("zig not found. Install Zig.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        if !project_dir.join("build.zig").exists() {
            return None;
        }

        Some(DetectionResult {
            language: "Zig".into(),
            framework: "zig test".into(),
            confidence: 0.95,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("zig");
        cmd.arg("build");
        cmd.arg("test");

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);

        let suites = parse_zig_output(&combined, exit_code);
        let duration = parse_zig_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse Zig test output.
///
/// Format:
/// ```text
/// Test [1/3] test.basic add... OK
/// Test [2/3] test.advanced... OK
/// Test [3/3] test.edge case... FAIL
/// 2 passed; 1 failed.
/// ```
/// Or:
/// ```text
/// All 3 tests passed.
/// ```
fn parse_zig_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // "Test [1/3] test.basic add... OK"
        if trimmed.starts_with("Test [") {
            let status = if trimmed.ends_with("OK") {
                TestStatus::Passed
            } else if trimmed.ends_with("FAIL") || trimmed.contains("FAIL") {
                TestStatus::Failed
            } else if trimmed.ends_with("SKIP") {
                TestStatus::Skipped
            } else {
                continue;
            };

            // Extract test name: between "] " and "..."
            let name = if let Some(bracket_end) = trimmed.find("] ") {
                let after = &trimmed[bracket_end + 2..];
                after
                    .rfind("...")
                    .map(|i| after[..i].trim())
                    .unwrap_or(after)
                    .to_string()
            } else {
                "unknown".into()
            };

            tests.push(TestCase {
                name,
                status,
                duration: Duration::from_millis(0),
                error: None,
            });
        }
    }

    // Fallback: parse summary
    if tests.is_empty()
        && let Some((passed, failed)) = parse_zig_summary(output)
    {
        for i in 0..passed {
            tests.push(TestCase {
                name: format!("test_{}", i + 1),
                status: TestStatus::Passed,
                duration: Duration::from_millis(0),
                error: None,
            });
        }
        for i in 0..failed {
            tests.push(TestCase {
                name: format!("failed_test_{}", i + 1),
                status: TestStatus::Failed,
                duration: Duration::from_millis(0),
                error: None,
            });
        }
    }

    if tests.is_empty() {
        tests.push(TestCase {
            name: "test_suite".into(),
            status: if exit_code == 0 {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            },
            duration: Duration::from_millis(0),
            error: None,
        });
    }

    vec![TestSuite {
        name: "tests".into(),
        tests,
    }]
}

fn parse_zig_summary(output: &str) -> Option<(usize, usize)> {
    for line in output.lines() {
        let trimmed = line.trim();

        // "All 3 tests passed."
        if trimmed.starts_with("All ") && trimmed.contains("passed") {
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            if words.len() >= 2 {
                let count: usize = words[1].parse().ok()?;
                return Some((count, 0));
            }
        }

        // "2 passed; 1 failed."
        if trimmed.contains("passed") && trimmed.contains("failed") {
            let mut passed = 0usize;
            let mut failed = 0usize;
            for part in trimmed.split(';') {
                let part = part.trim().trim_end_matches('.');
                let words: Vec<&str> = part.split_whitespace().collect();
                if words.len() >= 2 {
                    let count: usize = words[0].parse().unwrap_or(0);
                    if words[1].starts_with("passed") {
                        passed = count;
                    } else if words[1].starts_with("failed") {
                        failed = count;
                    }
                }
            }
            return Some((passed, failed));
        }
    }
    None
}

fn parse_zig_duration(output: &str) -> Option<Duration> {
    // Zig doesn't print total duration by default, but may print per-test timing
    // Some Zig setups show "time: 0.001s"
    for line in output.lines() {
        if let Some(idx) = line.find("time:") {
            let after = &line[idx + 5..];
            let num_str: String = after
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(secs) = num_str.parse::<f64>() {
                return Some(Duration::from_secs_f64(secs));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_zig_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("build.zig"),
            "const std = @import(\"std\");\n",
        )
        .unwrap();
        let adapter = ZigAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Zig");
        assert_eq!(det.framework, "zig test");
    }

    #[test]
    fn detect_no_zig() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = ZigAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_zig_detailed_output() {
        let stdout = r#"
Test [1/3] test.basic add... OK
Test [2/3] test.advanced... OK
Test [3/3] test.edge case... FAIL
2 passed; 1 failed.
"#;
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_zig_all_pass() {
        let stdout = r#"
Test [1/2] test.add... OK
Test [2/2] test.sub... OK
All 2 tests passed.
"#;
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_zig_summary_only() {
        let stdout = "All 5 tests passed.\n";
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 5);
        assert!(result.is_success());
    }

    #[test]
    fn parse_zig_summary_with_failures() {
        let stdout = "2 passed; 3 failed.\n";
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 3);
    }

    #[test]
    fn parse_zig_empty_output() {
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_zig_skipped_test() {
        let stdout = "Test [1/2] test.basic... OK\nTest [2/2] test.skip... SKIP\n";
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }
}
