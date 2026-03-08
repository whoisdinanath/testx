use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct GoAdapter;

impl Default for GoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GoAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl TestAdapter for GoAdapter {
    fn name(&self) -> &str {
        "Go"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("go").is_err() {
            Some("go".into())
        } else {
            None
        }
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        if !project_dir.join("go.mod").exists() {
            return None;
        }

        // Check for test files
        let has_tests = std::fs::read_dir(project_dir).ok()?.any(|entry| {
            entry
                .ok()
                .is_some_and(|e| e.file_name().to_string_lossy().ends_with("_test.go"))
        }) || find_test_files_recursive(project_dir);

        if !has_tests {
            return None;
        }

        Some(DetectionResult {
            language: "Go".into(),
            framework: "go test".into(),
            confidence: 0.95,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("go");
        cmd.arg("test");

        if extra_args.is_empty() {
            cmd.arg("-v"); // verbose for parsing individual tests
            cmd.arg("./..."); // all packages
        }

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);
        let failure_messages = parse_go_failures(&combined);
        let mut suites: Vec<TestSuite> = Vec::new();
        let mut current_pkg = String::new();
        let mut current_tests: Vec<TestCase> = Vec::new();

        for line in combined.lines() {
            let trimmed = line.trim();

            // Go test verbose output:
            // "=== RUN   TestFoo"
            // "--- PASS: TestFoo (0.00s)"
            // "--- FAIL: TestFoo (0.05s)"
            // "--- SKIP: TestFoo (0.00s)"

            if trimmed.starts_with("--- PASS:")
                || trimmed.starts_with("--- FAIL:")
                || trimmed.starts_with("--- SKIP:")
            {
                let status = if trimmed.starts_with("--- PASS:") {
                    TestStatus::Passed
                } else if trimmed.starts_with("--- FAIL:") {
                    TestStatus::Failed
                } else {
                    TestStatus::Skipped
                };

                let rest = trimmed.split(':').nth(1).unwrap_or("").trim();
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let name = parts.first().unwrap_or(&"unknown").to_string();
                let duration = parts
                    .get(1)
                    .and_then(|s| {
                        let s = s.trim_matches(|c| c == '(' || c == ')' || c == 's');
                        s.parse::<f64>().ok()
                    })
                    .map(Duration::from_secs_f64)
                    .unwrap_or(Duration::from_millis(0));

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

            // Package result line: "ok  	github.com/user/pkg	0.005s"
            // or: "FAIL	github.com/user/pkg	0.005s"
            if (trimmed.starts_with("ok") || trimmed.starts_with("FAIL")) && trimmed.contains('\t')
            {
                // Flush current tests to this new package suite
                let parts: Vec<&str> = trimmed.split('\t').collect();
                let pkg_name = parts.get(1).unwrap_or(&"").trim().to_string();

                if !current_tests.is_empty() {
                    suites.push(TestSuite {
                        name: if current_pkg.is_empty() {
                            pkg_name.clone()
                        } else {
                            current_pkg.clone()
                        },
                        tests: std::mem::take(&mut current_tests),
                    });
                }
                current_pkg = pkg_name;
            }
        }

        // Flush remaining
        if !current_tests.is_empty() {
            let name = if current_pkg.is_empty() {
                "tests".into()
            } else {
                current_pkg
            };
            suites.push(TestSuite {
                name,
                tests: current_tests,
            });
        }

        if suites.is_empty() {
            let status = if exit_code == 0 {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            suites.push(TestSuite {
                name: "tests".into(),
                tests: vec![TestCase {
                    name: "test_suite".into(),
                    status,
                    duration: Duration::from_millis(0),
                    error: None,
                }],
            });
        }

        // Parse total duration from last "ok" or "FAIL" line
        let duration = parse_go_total_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

fn find_test_files_recursive(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.to_string_lossy().ends_with("_test.go") {
            return true;
        }
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            // Skip hidden dirs and vendor
            if !name.starts_with('.')
                && name != "vendor"
                && name != "node_modules"
                && find_test_files_recursive(&path)
            {
                return true;
            }
        }
    }
    false
}

/// Parse go test failure output to extract error messages per test.
/// Go test verbose output shows errors as indented lines between `=== RUN` and `--- FAIL:`:
/// ```text
/// === RUN   TestDivide
///     math_test.go:15: expected 2, got 0
/// --- FAIL: TestDivide (0.00s)
/// ```
fn parse_go_failures(output: &str) -> std::collections::HashMap<String, String> {
    let mut failures = std::collections::HashMap::new();
    let lines: Vec<&str> = output.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "=== RUN   TestName"
        if let Some(rest) = trimmed.strip_prefix("=== RUN") {
            let test_name = rest.trim().to_string();
            if !test_name.is_empty() {
                let mut msg_lines = Vec::new();
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if l.starts_with("--- FAIL:")
                        || l.starts_with("--- PASS:")
                        || l.starts_with("--- SKIP:")
                        || l.starts_with("=== RUN")
                    {
                        break;
                    }
                    if !l.is_empty() {
                        msg_lines.push(l.to_string());
                    }
                    i += 1;
                }
                // Only store if this test actually failed
                if i < lines.len()
                    && lines[i].trim().starts_with("--- FAIL:")
                    && !msg_lines.is_empty()
                {
                    failures.insert(test_name, msg_lines.join(" | "));
                }
                continue;
            }
        }
        i += 1;
    }
    failures
}

fn parse_go_total_duration(output: &str) -> Option<Duration> {
    let mut total = Duration::from_secs(0);
    let mut found = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if (trimmed.starts_with("ok") || trimmed.starts_with("FAIL")) && trimmed.contains('\t') {
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if let Some(time_str) = parts.last() {
                let time_str = time_str.trim().trim_end_matches('s');
                if let Ok(secs) = time_str.parse::<f64>() {
                    total += Duration::from_secs_f64(secs);
                    found = true;
                }
            }
        }
    }
    if found { Some(total) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_go_verbose_output() {
        let stdout = r#"
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestSubtract
--- PASS: TestSubtract (0.00s)
=== RUN   TestDivide
    math_test.go:15: expected 2, got 0
--- FAIL: TestDivide (0.05s)
FAIL
FAIL	github.com/user/mathpkg	0.052s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());

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
                .contains("expected 2, got 0")
        );
    }

    #[test]
    fn parse_go_all_pass() {
        let stdout = r#"
=== RUN   TestHello
--- PASS: TestHello (0.00s)
=== RUN   TestWorld
--- PASS: TestWorld (0.01s)
ok  	github.com/user/pkg	0.015s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 0);
        assert!(result.is_success());
    }

    #[test]
    fn parse_go_skipped() {
        let stdout = r#"
=== RUN   TestFoo
--- SKIP: TestFoo (0.00s)
ok  	github.com/user/pkg	0.001s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_skipped(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_go_multiple_packages() {
        let stdout = r#"
=== RUN   TestA
--- PASS: TestA (0.00s)
ok  	github.com/user/pkg/a	0.005s
=== RUN   TestB
--- FAIL: TestB (0.02s)
FAIL	github.com/user/pkg/b	0.025s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_go_duration() {
        let output = "ok  \tgithub.com/user/pkg\t1.234s\n";
        let dur = parse_go_total_duration(output).unwrap();
        assert_eq!(dur, Duration::from_millis(1234));
    }

    #[test]
    fn detect_go_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example.com/test\n").unwrap();
        std::fs::write(dir.path().join("main_test.go"), "package main\n").unwrap();
        let adapter = GoAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "go test");
    }

    #[test]
    fn detect_no_go() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = GoAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_go_empty_output() {
        let adapter = GoAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_go_subtests() {
        let stdout = r#"
=== RUN   TestMath
=== RUN   TestMath/Add
--- PASS: TestMath/Add (0.00s)
=== RUN   TestMath/Subtract
--- PASS: TestMath/Subtract (0.00s)
--- PASS: TestMath (0.00s)
ok  	github.com/user/pkg	0.003s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        // Should capture parent and subtests
        assert!(result.total_passed() >= 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_go_panic_output() {
        let stdout = r#"
=== RUN   TestCrash
--- FAIL: TestCrash (0.00s)
panic: runtime error: index out of range [recovered]
FAIL	github.com/user/pkg	0.001s
"#;
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_go_no_test_files() {
        let stdout = "?   \tgithub.com/user/pkg\t[no test files]\n";
        let adapter = GoAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        // Should create a synthetic passing suite
        assert!(result.is_success());
    }

    #[test]
    fn detect_go_needs_test_files() {
        let dir = tempfile::tempdir().unwrap();
        // go.mod but no *_test.go files
        std::fs::write(dir.path().join("go.mod"), "module example.com/test\n").unwrap();
        std::fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let adapter = GoAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }
}
