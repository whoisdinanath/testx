use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct ElixirAdapter;

impl Default for ElixirAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ElixirAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl TestAdapter for ElixirAdapter {
    fn name(&self) -> &str {
        "Elixir"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("mix").is_err() {
            return Some("mix not found. Install Elixir.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        if !project_dir.join("mix.exs").exists() {
            return None;
        }

        Some(DetectionResult {
            language: "Elixir".into(),
            framework: "ExUnit".into(),
            confidence: 0.95,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("mix");
        cmd.arg("test");

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);

        let suites = parse_exunit_output(&combined, exit_code);
        let duration = parse_exunit_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse ExUnit output.
///
/// Format:
/// ```text
/// Compiling 1 file (.ex)
/// ...
///
///   1) test adds two numbers (MyApp.CalculatorTest)
///      test/calculator_test.exs:5
///      Assertion with == failed
///      left:  3
///      right: 4
///
/// Finished in 0.03 seconds (0.02s async, 0.01s sync)
/// 3 tests, 1 failure
/// ```
fn parse_exunit_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary line: "3 tests, 1 failure" or "3 tests, 1 failure, 1 excluded"
        // Also: "3 doctests, 3 tests, 0 failures"
        if (trimmed.contains("test") || trimmed.contains("doctest")) && trimmed.contains("failure")
        {
            let mut total = 0usize;
            let mut failures = 0usize;
            let mut excluded = 0usize;

            for part in trimmed.split(',') {
                let part = part.trim();
                let words: Vec<&str> = part.split_whitespace().collect();
                if words.len() >= 2 {
                    let count: usize = words[0].parse().unwrap_or(0);
                    if words[1].starts_with("test") || words[1].starts_with("doctest") {
                        total += count;
                    } else if words[1].starts_with("failure") {
                        failures = count;
                    } else if words[1].starts_with("excluded") || words[1].starts_with("skipped") {
                        excluded = count;
                    }
                }
            }

            if total > 0 || failures > 0 {
                let passed = total.saturating_sub(failures + excluded);
                for i in 0..passed {
                    tests.push(TestCase {
                        name: format!("test_{}", i + 1),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(0),
                        error: None,
                    });
                }
                for i in 0..failures {
                    tests.push(TestCase {
                        name: format!("failed_test_{}", i + 1),
                        status: TestStatus::Failed,
                        duration: Duration::from_millis(0),
                        error: None,
                    });
                }
                for i in 0..excluded {
                    tests.push(TestCase {
                        name: format!("excluded_test_{}", i + 1),
                        status: TestStatus::Skipped,
                        duration: Duration::from_millis(0),
                        error: None,
                    });
                }
                break;
            }
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

fn parse_exunit_duration(output: &str) -> Option<Duration> {
    // "Finished in 0.03 seconds (0.02s async, 0.01s sync)"
    for line in output.lines() {
        if line.contains("Finished in")
            && line.contains("second")
            && let Some(idx) = line.find("Finished in")
        {
            let after = &line[idx + 12..];
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
    fn detect_elixir_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mix.exs"),
            "defmodule MyApp.MixProject do\nend\n",
        )
        .unwrap();
        let adapter = ElixirAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Elixir");
        assert_eq!(det.framework, "ExUnit");
    }

    #[test]
    fn detect_no_elixir() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = ElixirAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_exunit_with_failures() {
        let stdout = r#"
Compiling 1 file (.ex)
..

  1) test adds two numbers (MyApp.CalculatorTest)
     test/calculator_test.exs:5
     Assertion with == failed

Finished in 0.03 seconds (0.02s async, 0.01s sync)
3 tests, 1 failure
"#;
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_exunit_all_pass() {
        let stdout = "Finished in 0.01 seconds\n5 tests, 0 failures\n";
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 5);
        assert!(result.is_success());
    }

    #[test]
    fn parse_exunit_with_excluded() {
        let stdout = "3 tests, 0 failures, 1 excluded\n";
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_exunit_with_doctests() {
        let stdout = "3 doctests, 5 tests, 0 failures\n";
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 8);
        assert_eq!(result.total_passed(), 8);
    }

    #[test]
    fn parse_exunit_empty_output() {
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_exunit_duration_test() {
        assert_eq!(
            parse_exunit_duration("Finished in 0.03 seconds (0.02s async, 0.01s sync)"),
            Some(Duration::from_millis(30))
        );
    }
}
