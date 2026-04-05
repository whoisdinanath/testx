use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::{combined_output, duration_from_secs_safe, ensure_non_empty};
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite,
};

pub struct RustAdapter;

impl Default for RustAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RustAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl TestAdapter for RustAdapter {
    fn name(&self) -> &str {
        "Rust"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("cargo").is_err() {
            Some("cargo".into())
        } else {
            None
        }
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let cargo_toml = project_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return None;
        }

        // Distinguish workspace roots from package roots
        let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
        let is_workspace = content.contains("[workspace]");
        let has_package = content.contains("[package]");

        // Pure workspace root with no [package] — cargo test still works
        // (runs all member tests) but confidence is lower
        let framework = if is_workspace && !has_package {
            "cargo test (workspace)"
        } else if is_workspace {
            "cargo test (workspace+package)"
        } else {
            "cargo test"
        };

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.20, project_dir.join("tests").is_dir())
            .signal(0.10, project_dir.join("Cargo.lock").exists())
            .signal(0.10, which::which("cargo").is_ok())
            .signal(0.05, project_dir.join("src").is_dir())
            // Pure workspace roots without src/ are less likely to be "the" test target
            .signal(
                -0.10,
                is_workspace && !has_package && !project_dir.join("src").is_dir(),
            )
            .finish();

        Some(DetectionResult {
            language: "Rust".into(),
            framework: framework.into(),
            confidence,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("cargo");
        cmd.arg("test");

        // Try to enable per-test timing on nightly (silently ignored on stable
        // since the caller's extra_args might conflict). We detect nightly by
        // probing `cargo +nightly` availability, but that's too expensive.
        // Instead we rely on users passing `-- -Z unstable-options --report-time`
        // manually if on nightly.

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn filter_args(&self, pattern: &str) -> Vec<String> {
        // cargo test uses positional args as substring/regex filters
        vec![pattern.to_string()]
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = combined_output(stdout, stderr);
        let mut suites: Vec<TestSuite> = Vec::new();
        let mut current_suite_name = String::from("tests");
        let mut current_tests: Vec<TestCase> = Vec::new();

        // First pass: collect failure messages
        let failure_messages = parse_cargo_failures(&combined);

        for line in combined.lines() {
            let trimmed = line.trim();

            // "running X tests" or "running X test"
            if trimmed.starts_with("running ")
                && (trimmed.ends_with(" tests") || trimmed.ends_with(" test"))
            {
                // If we had tests from a previous suite, flush
                if !current_tests.is_empty() {
                    suites.push(TestSuite {
                        name: current_suite_name.clone(),
                        tests: std::mem::take(&mut current_tests),
                    });
                }
                continue;
            }

            // "test result: ok. X passed; Y failed; Z ignored;"
            if trimmed.starts_with("test result:") {
                continue;
            }

            // "test module::test_name ... ok"
            // "test module::test_name ... ok <0.001s>"  (--report-time on nightly)
            // "test module::test_name ... FAILED"
            // "test module::test_name ... ignored"
            if let Some(without_prefix) = trimmed.strip_prefix("test ") {
                let (status, time_suffix) = if let Some(rest) = trimmed.strip_suffix(" ok") {
                    (Some(TestStatus::Passed), rest)
                } else if trimmed.ends_with(" FAILED") {
                    (Some(TestStatus::Failed), trimmed)
                } else if trimmed.ends_with(" ignored") {
                    (Some(TestStatus::Skipped), trimmed)
                } else {
                    (None, trimmed)
                };

                let Some(status) = status else {
                    continue;
                };

                // Parse test name — strip " ... ok" / " ... FAILED" / " ... ignored"
                let name = if let Some(idx) = without_prefix.rfind(" ... ") {
                    without_prefix[..idx].to_string()
                } else {
                    without_prefix.to_string()
                };

                // Extract module name as suite
                if let Some(last_sep) = name.rfind("::") {
                    current_suite_name = name[..last_sep].to_string();
                }

                // Try to parse per-test duration from --report-time output
                // Format: "test name ... ok <0.123s>"
                let duration = parse_report_time(time_suffix).unwrap_or(Duration::from_millis(0));

                // Attach error message if this test failed
                let error = if status == TestStatus::Failed {
                    failure_messages
                        .get(name.as_str())
                        .map(|msg| super::TestError {
                            message: msg.clone(),
                            location: None,
                        })
                } else {
                    None
                };

                current_tests.push(TestCase {
                    name,
                    status,
                    duration,
                    error,
                });
                continue;
            }

            // Compilation target: "Running unittests src/main.rs (target/debug/deps/testx-xxx)"
            if trimmed.starts_with("Running ") {
                if !current_tests.is_empty() {
                    suites.push(TestSuite {
                        name: current_suite_name.clone(),
                        tests: std::mem::take(&mut current_tests),
                    });
                }

                // Extract the source file as suite name
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    current_suite_name = parts[1..parts.len() - 1].join(" ");
                }
                continue;
            }
        }

        // Flush remaining
        if !current_tests.is_empty() {
            suites.push(TestSuite {
                name: current_suite_name,
                tests: current_tests,
            });
        }

        ensure_non_empty(&mut suites, exit_code, "tests");

        // Parse total duration from "test result: ... finished in X.XXs"
        let duration = parse_cargo_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

fn parse_cargo_duration(output: &str) -> Option<Duration> {
    // "test result: ok. 3 passed; 0 failed; 0 ignored; finished in 0.00s"
    // or just look for "finished in X.XXs"
    for line in output.lines() {
        if let Some(idx) = line.find("finished in ") {
            let after = &line[idx + 12..];
            let num_str: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(secs) = num_str.parse::<f64>() {
                return Some(duration_from_secs_safe(secs));
            }
        }
    }
    None
}

/// Parse per-test execution time from `--report-time` output (nightly).
/// Format: "test name ... ok <0.123s>" — we look for `<X.XXXs>` at line end.
fn parse_report_time(line: &str) -> Option<Duration> {
    let trimmed = line.trim();
    if let Some(start) = trimmed.rfind('<')
        && let Some(end) = trimmed.rfind('>')
        && start < end
    {
        let inner = &trimmed[start + 1..end];
        let num_str = inner.trim_end_matches('s');
        if let Ok(secs) = num_str.parse::<f64>() {
            return Some(duration_from_secs_safe(secs));
        }
    }
    None
}

/// Parse cargo test failure blocks to extract error messages per test.
/// Looks for patterns like:
/// ```text
/// ---- tests::test_name stdout ----
/// thread 'tests::test_name' panicked at 'assertion failed: ...'
/// ```
fn parse_cargo_failures(output: &str) -> std::collections::HashMap<&str, String> {
    let mut failures = std::collections::HashMap::new();
    let lines: Vec<&str> = output.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "---- test_name stdout ----"
        if trimmed.starts_with("---- ") && trimmed.ends_with(" stdout ----") && trimmed.len() > 17 {
            let test_name = &trimmed[5..trimmed.len() - 12].trim();
            // Collect the panic message from subsequent lines
            let mut msg_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if l.starts_with("---- ") || l == "failures:" || l.starts_with("test result:") {
                    break;
                }
                if l.starts_with("thread '") && l.contains("panicked at") {
                    // Extract just the panic message
                    if let Some(at_idx) = l.find("panicked at ") {
                        let msg = &l[at_idx + 12..];
                        let msg = msg.trim_matches('\'').trim_matches('"');
                        msg_lines.push(msg.to_string());
                    }
                } else if !l.is_empty()
                    && !l.starts_with("note:")
                    && !l.starts_with("stack backtrace:")
                {
                    msg_lines.push(l.to_string());
                }
                i += 1;
            }
            if !msg_lines.is_empty() {
                failures.insert(*test_name, msg_lines.join(" | "));
            }
            continue;
        }
        i += 1;
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cargo_test_output() {
        let stdout = r#"
running 3 tests
test tests::test_add ... ok
test tests::test_subtract ... ok
test tests::test_multiply ... FAILED

failures:

---- tests::test_multiply stdout ----
thread 'tests::test_multiply' panicked at 'assertion failed'

failures:
    tests::test_multiply

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output(stdout, "", 101);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
        assert_eq!(result.duration, Duration::from_millis(10));

        // Verify error message was captured
        let failed = &result.suites[0].failures();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].error.is_some());
        assert!(
            failed[0]
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("assertion failed")
        );
    }

    #[test]
    fn parse_cargo_all_pass() {
        let stdout = r#"
running 2 tests
test tests::test_a ... ok
test tests::test_b ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_cargo_with_ignored() {
        let stdout = r#"
running 3 tests
test tests::test_a ... ok
test tests::test_b ... ignored
test tests::test_c ... ok

test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_skipped(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_cargo_duration_extraction() {
        assert_eq!(
            parse_cargo_duration(
                "test result: ok. 2 passed; 0 failed; 0 ignored; finished in 1.23s"
            ),
            Some(Duration::from_millis(1230))
        );
        assert_eq!(parse_cargo_duration("no duration"), None);
    }

    #[test]
    fn parse_cargo_empty_output() {
        let adapter = RustAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn detect_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let adapter = RustAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "cargo test");
    }

    #[test]
    fn detect_no_rust() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = RustAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_cargo_multiple_targets() {
        let stdout = r#"
   Compiling testx v0.1.0
     Running unittests src/lib.rs (target/debug/deps/testx-abc123)

running 2 tests
test lib_test_a ... ok
test lib_test_b ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; finished in 0.01s

     Running unittests src/main.rs (target/debug/deps/testx-def456)

running 1 test
test main_test ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; finished in 0.00s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 3);
        assert!(result.is_success());
        assert!(result.suites.len() >= 2);
    }

    #[test]
    fn parse_cargo_all_failures() {
        let stdout = r#"
running 2 tests
test tests::test_x ... FAILED
test tests::test_y ... FAILED

failures:

---- tests::test_x stdout ----
thread 'tests::test_x' panicked at 'not yet implemented'

---- tests::test_y stdout ----
thread 'tests::test_y' panicked at 'todo'

failures:
    tests::test_x
    tests::test_y

test result: FAILED. 0 passed; 2 failed; 0 ignored; finished in 0.02s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output(stdout, "", 101);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_failed(), 2);
        assert_eq!(result.total_passed(), 0);
        assert!(!result.is_success());

        // Both should have error messages
        let suite = &result.suites[0];
        for tc in suite.failures() {
            assert!(tc.error.is_some());
        }
    }

    #[test]
    fn parse_cargo_stderr_output() {
        // Some output goes to stderr (compilation messages)
        let stderr = r#"
running 1 test
test basic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; finished in 0.00s
"#;
        let adapter = RustAdapter::new();
        let result = adapter.parse_output("", stderr, 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn slowest_tests_ordering() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "tests".into(),
                tests: vec![
                    TestCase {
                        name: "fast".into(),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(10),
                        error: None,
                    },
                    TestCase {
                        name: "slow".into(),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(500),
                        error: None,
                    },
                    TestCase {
                        name: "medium".into(),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(100),
                        error: None,
                    },
                ],
            }],
            duration: Duration::from_millis(610),
            raw_exit_code: 0,
        };

        let slowest = result.slowest_tests(2);
        assert_eq!(slowest.len(), 2);
        assert_eq!(slowest[0].1.name, "slow");
        assert_eq!(slowest[1].1.name, "medium");
    }

    #[test]
    fn test_suite_helpers() {
        let suite = TestSuite {
            name: "test".into(),
            tests: vec![
                TestCase {
                    name: "a".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "b".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: Some(crate::adapters::TestError {
                        message: "boom".into(),
                        location: None,
                    }),
                },
                TestCase {
                    name: "c".into(),
                    status: TestStatus::Skipped,
                    duration: Duration::from_millis(0),
                    error: None,
                },
            ],
        };

        assert_eq!(suite.passed(), 1);
        assert_eq!(suite.failed(), 1);
        assert_eq!(suite.skipped(), 1);
        assert!(!suite.is_passed());
        assert_eq!(suite.failures().len(), 1);
        assert_eq!(suite.failures()[0].name, "b");
    }
}
