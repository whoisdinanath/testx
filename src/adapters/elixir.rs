use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{
    DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus, TestSuite,
};

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

        // Try verbose/trace parsing first
        let trace_tests = parse_exunit_trace(&combined);
        let suites = if trace_tests.iter().any(|s| !s.tests.is_empty()) {
            trace_tests
        } else {
            parse_exunit_output(&combined, exit_code)
        };

        // Enrich with failure details
        let failures = parse_exunit_failures(&combined);
        let suites = enrich_exunit_errors(suites, &failures);

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
                return Some(duration_from_secs_safe(secs));
            }
        }
    }
    None
}

// ─── ExUnit --trace Verbose Parser ──────────────────────────────────────────

/// Parse ExUnit `--trace` output.
///
/// ```text
///   * test greets the world (0.00ms) [L#4]
///   * test adds two numbers (0.01ms) [L#8]
///   * test handles nil input (1.2ms) [L#12]
/// ```
fn parse_exunit_trace(output: &str) -> Vec<TestSuite> {
    let mut suites_map: std::collections::HashMap<String, Vec<TestCase>> =
        std::collections::HashMap::new();
    let mut current_module = String::from("tests");

    for line in output.lines() {
        let trimmed = line.trim();

        // Module header line: "MyApp.CalculatorTest [test/calculator_test.exs]"
        if !trimmed.starts_with('*')
            && !trimmed.is_empty()
            && trimmed.contains('[')
            && trimmed.contains("test/")
        {
            if let Some(bracket_idx) = trimmed.find('[') {
                current_module = trimmed[..bracket_idx].trim().to_string();
            }
            continue;
        }

        // Test line: "  * test greets the world (0.00ms) [L#4]"
        if let Some(rest) = trimmed.strip_prefix("* test ") {
            let (name, duration, status) = parse_trace_test_line(rest);

            suites_map
                .entry(current_module.clone())
                .or_default()
                .push(TestCase {
                    name,
                    status,
                    duration,
                    error: None,
                });
        }
        // Doctest line: "  * doctest MyApp.Calculator.add/2 (1) (0.00ms) [L#3]"
        else if let Some(rest) = trimmed.strip_prefix("* doctest ") {
            let (name, duration, status) = parse_trace_test_line(rest);

            suites_map
                .entry(current_module.clone())
                .or_default()
                .push(TestCase {
                    name: format!("doctest {}", name),
                    status,
                    duration,
                    error: None,
                });
        }
    }

    let mut suites: Vec<TestSuite> = suites_map
        .into_iter()
        .map(|(name, tests)| TestSuite { name, tests })
        .collect();
    suites.sort_by(|a, b| a.name.cmp(&b.name));

    suites
}

/// Parse a trace test line after "* test ".
/// Input: "greets the world (0.00ms) [L#4]"
/// Returns: (name, duration, status)
fn parse_trace_test_line(s: &str) -> (String, Duration, TestStatus) {
    // Check for "(excluded)" marker
    if s.contains("(excluded)") {
        let name = s.split("(excluded)").next().unwrap_or(s).trim().to_string();
        return (name, Duration::from_millis(0), TestStatus::Skipped);
    }

    // Extract duration from "(0.01ms)" or "(1.2ms)"
    let mut name = s.to_string();
    let mut duration = Duration::from_millis(0);
    let mut status = TestStatus::Passed;

    if let Some(paren_start) = s.find('(')
        && let Some(paren_end) = s[paren_start..].find(')')
    {
        let time_str = &s[paren_start + 1..paren_start + paren_end];

        if let Some(num) = time_str.strip_suffix("ms")
            && let Ok(ms) = num.parse::<f64>()
        {
            duration = duration_from_secs_safe(ms / 1000.0);
        }

        name = s[..paren_start].trim().to_string();
    }

    // Remove trailing "[L#N]" location marker
    if let Some(bracket_idx) = name.rfind('[') {
        name = name[..bracket_idx].trim().to_string();
    }

    // Check for failure marker in the original line
    // Failed tests in trace mode still show duration but are followed by failure blocks
    // We mark them as passed here; failures are enriched separately
    if s.contains("** (ExUnit.AssertionError)") {
        status = TestStatus::Failed;
    }

    (name, duration, status)
}

// ─── ExUnit Failure Block Parser ────────────────────────────────────────────

/// A parsed failure from ExUnit output.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ExUnitFailure {
    /// Test name
    name: String,
    /// Module name
    module: String,
    /// Error message
    message: String,
    /// Source location
    location: Option<String>,
}

/// Parse ExUnit failure blocks.
///
/// ```text
///   1) test adds two numbers (MyApp.CalculatorTest)
///      test/calculator_test.exs:5
///      Assertion with == failed
///      code:  assert 1 + 1 == 3
///      left:  2
///      right: 3
/// ```
fn parse_exunit_failures(output: &str) -> Vec<ExUnitFailure> {
    let mut failures = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_module = String::new();
    let mut current_message = Vec::new();
    let mut current_location: Option<String> = None;
    let mut in_failure = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Failure header: "  1) test adds two numbers (MyApp.CalculatorTest)"
        if let Some((num_rest, module_paren)) = parse_exunit_failure_header(trimmed) {
            // Save previous
            if let Some(name) = current_name.take() {
                failures.push(ExUnitFailure {
                    name,
                    module: current_module.clone(),
                    message: current_message.join("\n").trim().to_string(),
                    location: current_location.take(),
                });
            }

            current_name = Some(num_rest);
            current_module = module_paren;
            current_message.clear();
            current_location = None;
            in_failure = true;
            continue;
        }

        if in_failure {
            // Location line: "     test/calculator_test.exs:5"
            if trimmed.starts_with("test/") || trimmed.starts_with("lib/") {
                current_location = Some(trimmed.to_string());
            }
            // End of failure block (empty line or next numbered failure)
            else if trimmed.is_empty() && !current_message.is_empty() {
                if let Some(name) = current_name.take() {
                    failures.push(ExUnitFailure {
                        name,
                        module: current_module.clone(),
                        message: current_message.join("\n").trim().to_string(),
                        location: current_location.take(),
                    });
                }
                in_failure = false;
                current_message.clear();
            } else if trimmed.starts_with("Finished in") {
                if let Some(name) = current_name.take() {
                    failures.push(ExUnitFailure {
                        name,
                        module: current_module.clone(),
                        message: current_message.join("\n").trim().to_string(),
                        location: current_location.take(),
                    });
                }
                break;
            } else if !trimmed.is_empty() {
                current_message.push(trimmed.to_string());
            }
        }
    }

    // Save last
    if let Some(name) = current_name {
        failures.push(ExUnitFailure {
            name,
            module: current_module,
            message: current_message.join("\n").trim().to_string(),
            location: current_location,
        });
    }

    failures
}

/// Parse failure header like "1) test adds two numbers (MyApp.CalculatorTest)".
/// Returns (test_name, module_name).
fn parse_exunit_failure_header(line: &str) -> Option<(String, String)> {
    // Must start with a digit
    let first = line.chars().next()?;
    if !first.is_ascii_digit() {
        return None;
    }

    // Find ") test " or ") doctest "
    let test_marker = if line.contains(") test ") {
        ") test "
    } else if line.contains(") doctest ") {
        ") doctest "
    } else {
        return None;
    };

    let marker_idx = line.find(test_marker)?;
    let after_marker = &line[marker_idx + test_marker.len()..];

    // Extract module from trailing parentheses
    if let Some(paren_start) = after_marker.rfind('(') {
        let name = after_marker[..paren_start].trim().to_string();
        let module = after_marker[paren_start + 1..]
            .trim_end_matches(')')
            .to_string();
        Some((name, module))
    } else {
        Some((after_marker.trim().to_string(), String::new()))
    }
}

/// Enrich ExUnit test cases with error details from failure blocks.
fn enrich_exunit_errors(suites: Vec<TestSuite>, failures: &[ExUnitFailure]) -> Vec<TestSuite> {
    suites
        .into_iter()
        .map(|suite| {
            let tests = suite
                .tests
                .into_iter()
                .map(|mut test| {
                    // Check if this test has a matching failure
                    if let Some(failure) = failures
                        .iter()
                        .find(|f| f.name.contains(&test.name) || test.name.contains(&f.name))
                    {
                        // Mark as failed if it was parsed as passed but has a failure
                        test.status = TestStatus::Failed;
                        if test.error.is_none() {
                            test.error = Some(TestError {
                                message: if failure.message.len() > 500 {
                                    format!("{}...", &failure.message[..500])
                                } else {
                                    failure.message.clone()
                                },
                                location: failure.location.clone(),
                            });
                        }
                    }
                    test
                })
                .collect();
            TestSuite {
                name: suite.name,
                tests,
            }
        })
        .collect()
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

    // ─── Trace Parser Tests ─────────────────────────────────────────────

    #[test]
    fn parse_exunit_trace_basic() {
        let output = r#"
MyApp.CalculatorTest [test/calculator_test.exs]
  * test greets the world (0.00ms) [L#4]
  * test adds two numbers (0.01ms) [L#8]
  * test handles nil input (1.2ms) [L#12]

Finished in 0.02 seconds
3 tests, 0 failures
"#;
        let suites = parse_exunit_trace(output);
        assert!(!suites.is_empty());

        let suite = &suites[0];
        assert_eq!(suite.tests.len(), 3);
        assert_eq!(suite.tests[0].name, "greets the world");
        assert_eq!(suite.tests[1].name, "adds two numbers");
    }

    #[test]
    fn parse_exunit_trace_with_excluded() {
        let output = "  * test slow test (excluded) [L#20]\n  * test fast test (0.01ms) [L#5]\n";
        let suites = parse_exunit_trace(output);
        let all_tests: Vec<_> = suites.iter().flat_map(|s| &s.tests).collect();

        let excluded: Vec<_> = all_tests
            .iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .collect();
        assert_eq!(excluded.len(), 1);
    }

    #[test]
    fn parse_trace_test_line_with_duration() {
        let (name, dur, status) = parse_trace_test_line("greets the world (0.50ms) [L#4]");
        assert_eq!(name, "greets the world");
        assert_eq!(status, TestStatus::Passed);
        assert!(dur.as_micros() >= 490);
    }

    #[test]
    fn parse_trace_test_line_excluded() {
        let (name, _dur, status) = parse_trace_test_line("slow test (excluded) [L#20]");
        assert_eq!(name, "slow test");
        assert_eq!(status, TestStatus::Skipped);
    }

    #[test]
    fn parse_exunit_trace_doctest() {
        let output = "  * doctest MyApp.Calculator.add/2 (1) (0.01ms) [L#3]\n";
        let suites = parse_exunit_trace(output);
        let all_tests: Vec<_> = suites.iter().flat_map(|s| &s.tests).collect();
        assert_eq!(all_tests.len(), 1);
        assert!(all_tests[0].name.starts_with("doctest"));
    }

    // ─── Failure Extraction Tests ────────────────────────────────────────

    #[test]
    fn parse_exunit_failure_blocks() {
        let output = r#"
  1) test adds two numbers (MyApp.CalculatorTest)
     test/calculator_test.exs:5
     Assertion with == failed
     code:  assert 1 + 1 == 3
     left:  2
     right: 3

  2) test subtracts (MyApp.CalculatorTest)
     test/calculator_test.exs:10
     Assertion with == failed
     left:  5
     right: 3

Finished in 0.03 seconds
"#;
        let failures = parse_exunit_failures(output);
        assert_eq!(failures.len(), 2);

        assert_eq!(failures[0].name, "adds two numbers");
        assert_eq!(failures[0].module, "MyApp.CalculatorTest");
        assert!(failures[0].message.contains("Assertion with == failed"));
        assert_eq!(
            failures[0].location.as_ref().unwrap(),
            "test/calculator_test.exs:5"
        );

        assert_eq!(failures[1].name, "subtracts");
    }

    #[test]
    fn parse_exunit_failure_header_parsing() {
        let result = parse_exunit_failure_header("1) test adds numbers (MyApp.CalcTest)");
        assert!(result.is_some());
        let (name, module) = result.unwrap();
        assert_eq!(name, "adds numbers");
        assert_eq!(module, "MyApp.CalcTest");
    }

    #[test]
    fn parse_exunit_failure_header_no_match() {
        assert!(parse_exunit_failure_header("not a failure header").is_none());
        assert!(parse_exunit_failure_header("Finished in 0.03 seconds").is_none());
    }

    #[test]
    fn parse_exunit_failures_empty() {
        let output = "Finished in 0.01 seconds\n5 tests, 0 failures\n";
        let failures = parse_exunit_failures(output);
        assert!(failures.is_empty());
    }

    // ─── Integration Tests ──────────────────────────────────────────────

    #[test]
    fn full_exunit_trace_with_failures() {
        let stdout = r#"
MyApp.CalculatorTest [test/calculator_test.exs]
  * test adds two numbers (0.01ms) [L#4]
  * test subtracts (0.01ms) [L#8]

  1) test adds two numbers (MyApp.CalculatorTest)
     test/calculator_test.exs:5
     Assertion with == failed
     left:  2
     right: 3

Finished in 0.03 seconds (0.02s async, 0.01s sync)
2 tests, 1 failure
"#;
        let adapter = ElixirAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn enrich_exunit_error_details() {
        let suites = vec![TestSuite {
            name: "tests".into(),
            tests: vec![TestCase {
                name: "failed_test_1".into(),
                status: TestStatus::Failed,
                duration: Duration::from_millis(0),
                error: None,
            }],
        }];

        let failures = vec![ExUnitFailure {
            name: "failed_test_1".to_string(),
            module: "MyApp.Test".to_string(),
            message: "Assertion failed".to_string(),
            location: Some("test/my_test.exs:5".to_string()),
        }];

        let enriched = enrich_exunit_errors(suites, &failures);
        let test = &enriched[0].tests[0];
        assert!(test.error.is_some());
        assert!(
            test.error
                .as_ref()
                .unwrap()
                .message
                .contains("Assertion failed")
        );
    }

    #[test]
    fn parse_exunit_trace_multiple_modules() {
        let output = r#"
MyApp.UserTest [test/user_test.exs]
  * test create user (0.01ms) [L#4]

MyApp.AdminTest [test/admin_test.exs]
  * test admin access (0.02ms) [L#4]
"#;
        let suites = parse_exunit_trace(output);
        assert_eq!(suites.len(), 2);
    }
}
