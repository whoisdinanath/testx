use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct PythonAdapter;

impl Default for PythonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Check if pytest is the test framework
    fn is_pytest(project_dir: &Path) -> bool {
        // Check for pytest-specific files/configs
        let markers = ["pytest.ini", ".pytest_cache", "conftest.py"];
        for m in &markers {
            if project_dir.join(m).exists() {
                return true;
            }
        }

        // Check pyproject.toml for pytest config
        let pyproject = project_dir.join("pyproject.toml");
        if pyproject.exists()
            && let Ok(content) = std::fs::read_to_string(&pyproject)
            && (content.contains("[tool.pytest") || content.contains("pytest"))
        {
            return true;
        }

        // Check setup.cfg
        let setup_cfg = project_dir.join("setup.cfg");
        if setup_cfg.exists()
            && let Ok(content) = std::fs::read_to_string(&setup_cfg)
            && content.contains("[tool:pytest]")
        {
            return true;
        }

        // Check tox.ini
        let tox_ini = project_dir.join("tox.ini");
        if tox_ini.exists()
            && let Ok(content) = std::fs::read_to_string(&tox_ini)
            && content.contains("[pytest]")
        {
            return true;
        }

        false
    }

    /// Check if Django is present
    fn is_django(project_dir: &Path) -> bool {
        project_dir.join("manage.py").exists()
    }

    /// Detect the Python package manager to use as a prefix
    fn detect_runner_prefix(project_dir: &Path) -> Option<Vec<String>> {
        if project_dir.join("uv.lock").exists() || {
            let pyproject = project_dir.join("pyproject.toml");
            pyproject.exists()
                && std::fs::read_to_string(&pyproject)
                    .map(|c| c.contains("[tool.uv]"))
                    .unwrap_or(false)
        } {
            return Some(vec!["uv".into(), "run".into()]);
        }
        if project_dir.join("poetry.lock").exists() {
            return Some(vec!["poetry".into(), "run".into()]);
        }
        if project_dir.join("pdm.lock").exists() {
            return Some(vec!["pdm".into(), "run".into()]);
        }
        None
    }
}

impl TestAdapter for PythonAdapter {
    fn name(&self) -> &str {
        "Python"
    }

    fn check_runner(&self) -> Option<String> {
        // If uv/poetry/pdm is the runner, check that instead
        for runner in ["uv", "poetry", "pdm", "pytest", "python"] {
            if which::which(runner).is_ok() {
                return None;
            }
        }
        Some("python".into())
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let has_python_files = project_dir.join("pyproject.toml").exists()
            || project_dir.join("setup.py").exists()
            || project_dir.join("setup.cfg").exists()
            || project_dir.join("requirements.txt").exists()
            || project_dir.join("Pipfile").exists();

        if !has_python_files {
            return None;
        }

        let framework = if Self::is_pytest(project_dir) {
            "pytest"
        } else if Self::is_django(project_dir) {
            "django"
        } else {
            "unittest"
        };

        Some(DetectionResult {
            language: "Python".into(),
            framework: framework.into(),
            confidence: if Self::is_pytest(project_dir) {
                0.95
            } else {
                0.7
            },
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let prefix = Self::detect_runner_prefix(project_dir);
        let is_pytest = Self::is_pytest(project_dir);
        let is_django = Self::is_django(project_dir);

        let mut cmd;

        if let Some(prefix_args) = &prefix {
            cmd = Command::new(&prefix_args[0]);
            for arg in &prefix_args[1..] {
                cmd.arg(arg);
            }
            if is_pytest {
                cmd.arg("pytest");
            } else if is_django {
                cmd.arg("python").arg("-m").arg("django").arg("test");
            } else {
                cmd.arg("python").arg("-m").arg("unittest");
            }
        } else if is_pytest {
            cmd = Command::new("pytest");
        } else if is_django {
            cmd = Command::new("python");
            cmd.arg("manage.py").arg("test");
        } else {
            cmd = Command::new("python");
            cmd.arg("-m").arg("unittest");
        }

        // Add verbose flag for better output parsing (pytest)
        if is_pytest && extra_args.is_empty() {
            cmd.arg("-v");
        }

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);
        let failure_messages = parse_pytest_failures(&combined);
        let mut suites: Vec<TestSuite> = Vec::new();
        let mut current_suite_name = String::from("tests");
        let mut tests: Vec<TestCase> = Vec::new();

        for line in combined.lines() {
            let trimmed = line.trim();

            // pytest verbose output: "test_file.py::TestClass::test_name PASSED"
            // or: "test_file.py::test_name PASSED"
            if let Some((test_path, status_str)) = parse_pytest_line(trimmed) {
                let parts: Vec<&str> = test_path.split("::").collect();
                let suite_name = parts.first().unwrap_or(&"tests").to_string();
                let test_name = parts.last().unwrap_or(&"unknown").to_string();

                // If suite changed, flush current tests
                if suite_name != current_suite_name && !tests.is_empty() {
                    suites.push(TestSuite {
                        name: current_suite_name.clone(),
                        tests: std::mem::take(&mut tests),
                    });
                }
                current_suite_name = suite_name;

                let status = match status_str.to_uppercase().as_str() {
                    "PASSED" => TestStatus::Passed,
                    "FAILED" => TestStatus::Failed,
                    "SKIPPED" | "XFAIL" | "XPASS" => TestStatus::Skipped,
                    "ERROR" => TestStatus::Failed,
                    _ => TestStatus::Failed,
                };

                let error = if status == TestStatus::Failed {
                    // Try full path first, then just test name
                    failure_messages
                        .get(&test_path)
                        .or_else(|| failure_messages.get(&test_name))
                        .map(|msg| super::TestError {
                            message: msg.clone(),
                            location: None,
                        })
                } else {
                    None
                };

                tests.push(TestCase {
                    name: test_name,
                    status,
                    duration: Duration::from_millis(0),
                    error,
                });
            }
        }

        // Flush remaining tests
        if !tests.is_empty() {
            suites.push(TestSuite {
                name: current_suite_name,
                tests,
            });
        }

        // If we couldn't parse any individual tests, create a summary suite from the summary line
        if suites.is_empty() {
            suites.push(parse_pytest_summary(&combined, exit_code));
        }

        // Try to parse total duration from pytest summary
        let duration = parse_pytest_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse a pytest verbose output line like "tests/test_foo.py::test_bar PASSED"
fn parse_pytest_line(line: &str) -> Option<(String, String)> {
    // Match patterns like: "path::test_name PASSED  [ 50%]"
    let statuses = ["PASSED", "FAILED", "SKIPPED", "ERROR", "XFAIL", "XPASS"];
    for status in &statuses {
        if let Some(idx) = line.rfind(status) {
            // Ensure the status word is preceded by whitespace (not part of test name)
            if idx > 0 && !line.as_bytes()[idx - 1].is_ascii_whitespace() {
                continue;
            }
            let path = line[..idx].trim().to_string();
            if path.contains("::") {
                return Some((path, status.to_string()));
            }
        }
    }
    None
}

/// Parse pytest summary line like "=== 5 passed, 2 failed in 0.32s ==="
fn parse_pytest_summary(output: &str, exit_code: i32) -> TestSuite {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    for line in output.lines() {
        let trimmed = line.trim().trim_matches('=').trim();
        if trimmed.contains("passed") || trimmed.contains("failed") || trimmed.contains("error") {
            // Parse "5 passed", "2 failed", etc.
            for part in trimmed.split(',') {
                let part = part.trim();
                if let Some(n) = part
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if part.contains("passed") {
                        passed = n;
                    } else if part.contains("failed") || part.contains("error") {
                        failed = n;
                    } else if part.contains("skipped") {
                        skipped = n;
                    }
                }
            }
        }
    }

    let mut tests = Vec::new();
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
    for i in 0..skipped {
        tests.push(TestCase {
            name: format!("skipped_test_{}", i + 1),
            status: TestStatus::Skipped,
            duration: Duration::from_millis(0),
            error: None,
        });
    }

    // If we still got nothing, infer from exit code
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

    TestSuite {
        name: "tests".into(),
        tests,
    }
}

/// Parse pytest FAILURES section to extract error messages per test.
/// Pytest output looks like:
/// ```text
/// =========================== FAILURES ===========================
/// __________________ test_multiply __________________
///
///     def test_multiply():
/// >       assert multiply(2, 3) == 7
/// E       assert 6 == 7
/// E       +  where 6 = multiply(2, 3)
///
/// tests/test_math.py:10: AssertionError
/// =========================== short test summary info ===========================
/// ```
fn parse_pytest_failures(output: &str) -> std::collections::HashMap<String, String> {
    let mut failures = std::collections::HashMap::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut in_failures = false;

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Enter FAILURES section
        if trimmed.contains("FAILURES") && trimmed.starts_with('=') {
            in_failures = true;
            i += 1;
            continue;
        }

        // Exit FAILURES section
        if in_failures
            && trimmed.starts_with('=')
            && (trimmed.contains("short test summary")
                || trimmed.contains("passed")
                || trimmed.contains("failed")
                || trimmed.contains("error"))
        {
            break;
        }

        // Match test header: "__________________ test_name __________________"
        if in_failures && trimmed.starts_with('_') && trimmed.ends_with('_') {
            let test_name = trimmed.trim_matches('_').trim().to_string();
            if !test_name.is_empty() {
                let mut error_lines = Vec::new();
                let mut location = None;
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    // Next test header or section boundary
                    if (l.starts_with('_') && l.ends_with('_') && l.len() > 5)
                        || (l.starts_with('=') && l.len() > 5)
                    {
                        break;
                    }
                    // Assertion lines start with "E"
                    if l.starts_with("E ") || l.starts_with("E\t") {
                        error_lines.push(l[1..].trim().to_string());
                    }
                    // Location line like "tests/test_math.py:10: AssertionError"
                    if l.contains(".py:")
                        && l.contains(':')
                        && !l.starts_with('>')
                        && !l.starts_with("E")
                    {
                        let parts: Vec<&str> = l.splitn(3, ':').collect();
                        if parts.len() >= 2 {
                            location = Some(format!("{}:{}", parts[0].trim(), parts[1].trim()));
                        }
                    }
                    i += 1;
                }
                if !error_lines.is_empty() {
                    let mut msg = error_lines.join(" | ");
                    if let Some(loc) = location {
                        msg = format!("{} ({})", msg, loc);
                    }
                    failures.insert(test_name, msg);
                }
                continue;
            }
        }
        i += 1;
    }
    failures
}

/// Parse duration from pytest summary like "in 0.32s"
fn parse_pytest_duration(output: &str) -> Option<Duration> {
    for line in output.lines() {
        if let Some(idx) = line.find(" in ") {
            let after = &line[idx + 4..];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pytest_verbose_output() {
        let stdout = r#"
============================= test session starts ==============================
collected 4 items

tests/test_math.py::test_add PASSED                                      [ 25%]
tests/test_math.py::test_subtract PASSED                                 [ 50%]
tests/test_math.py::test_multiply FAILED                                 [ 75%]
tests/test_string.py::test_upper PASSED                                  [100%]

=================================== FAILURES ===================================
________________________________ test_multiply _________________________________

    def test_multiply():
>       assert multiply(2, 3) == 7
E       assert 6 == 7
E         +  where 6 = multiply(2, 3)

tests/test_math.py:10: AssertionError
=========================== short test summary info ============================
FAILED tests/test_math.py::test_multiply - assert 6 == 7
============================== 3 passed, 1 failed in 0.12s =====================
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 4);
        assert_eq!(result.total_passed(), 3);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
        assert_eq!(result.suites.len(), 2); // two test files
        assert_eq!(result.duration, Duration::from_millis(120));

        // Verify error message was captured
        let failed: Vec<_> = result.suites.iter().flat_map(|s| s.failures()).collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].error.is_some());
        assert!(
            failed[0]
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("assert 6 == 7")
        );
    }

    #[test]
    fn parse_pytest_all_pass() {
        let stdout = "========================= 5 passed in 0.32s =========================\n";
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 5);
        assert!(result.is_success());
    }

    #[test]
    fn parse_pytest_with_skipped() {
        let stdout = r#"
tests/test_foo.py::test_a PASSED
tests/test_foo.py::test_b SKIPPED
tests/test_foo.py::test_c PASSED

========================= 2 passed, 1 skipped in 0.05s =========================
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_skipped(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_pytest_class_based() {
        let stdout = r#"
tests/test_calc.py::TestCalculator::test_add PASSED
tests/test_calc.py::TestCalculator::test_div FAILED
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_pytest_summary_only() {
        let stdout = "===== 10 passed, 2 failed, 3 skipped in 1.50s =====\n";
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_passed(), 10);
        assert_eq!(result.total_failed(), 2);
        assert_eq!(result.total_skipped(), 3);
        assert_eq!(result.total_tests(), 15);
    }

    #[test]
    fn parse_pytest_duration_extraction() {
        assert_eq!(
            parse_pytest_duration("=== 1 passed in 2.34s ==="),
            Some(Duration::from_millis(2340))
        );
        assert_eq!(parse_pytest_duration("no duration here"), None);
    }

    #[test]
    fn parse_pytest_line_function() {
        assert_eq!(
            parse_pytest_line("tests/test_foo.py::test_bar PASSED                    [ 50%]"),
            Some(("tests/test_foo.py::test_bar".into(), "PASSED".into()))
        );
        assert_eq!(parse_pytest_line("collected 5 items"), None);
        assert_eq!(parse_pytest_line(""), None);
    }

    #[test]
    fn detect_in_pytest_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .unwrap();
        let adapter = PythonAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "pytest");
        assert!(det.confidence > 0.9);
    }

    #[test]
    fn detect_no_python() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let adapter = PythonAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn detect_django_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "django\n").unwrap();
        std::fs::write(dir.path().join("manage.py"), "#!/usr/bin/env python\n").unwrap();
        let adapter = PythonAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "django");
    }

    #[test]
    fn parse_pytest_empty_output() {
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_pytest_xfail_xpass() {
        let stdout = r#"
tests/test_edge.py::test_expected_fail XFAIL
tests/test_edge.py::test_unexpected_pass XPASS

========================= 2 xfailed in 0.05s =========================
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        // XFAIL and XPASS should be counted as skipped
        assert_eq!(result.total_skipped(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_pytest_parametrized() {
        let stdout = r#"
tests/test_math.py::test_add[1-2-3] PASSED
tests/test_math.py::test_add[0-0-0] PASSED
tests/test_math.py::test_add[-1-1-0] PASSED

========================= 3 passed in 0.01s =========================
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 3);
    }

    #[test]
    fn parse_pytest_error_status() {
        let stdout = r#"
tests/test_math.py::test_setup ERROR

========================= 1 error in 0.10s =========================
"#;
        let adapter = PythonAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn detect_pipfile_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Pipfile"), "[packages]\n").unwrap();
        let adapter = PythonAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Python");
    }

    #[test]
    fn detect_unittest_fallback() {
        // Has Python markers but no pytest/django markers
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("setup.py"),
            "from setuptools import setup\n",
        )
        .unwrap();
        let adapter = PythonAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "unittest");
        assert!(det.confidence < 0.8);
    }
}
