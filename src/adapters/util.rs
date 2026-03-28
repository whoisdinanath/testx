use std::process::Command;
use std::time::Duration;

use crate::adapters::{DetectionResult, TestCase, TestError, TestRunResult, TestStatus, TestSuite};

/// Create a Duration from seconds, returning Duration::ZERO for NaN, infinity, or negative values.
/// This is a safe wrapper around `Duration::from_secs_f64` which panics on such inputs.
pub fn duration_from_secs_safe(secs: f64) -> Duration {
    if secs.is_finite() && secs >= 0.0 {
        Duration::from_secs_f64(secs)
    } else {
        Duration::ZERO
    }
}

/// Combine stdout and stderr into a single string for parsing.
pub fn combined_output(stdout: &str, stderr: &str) -> String {
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    if stdout.is_empty() {
        return stderr.to_string();
    }
    if stderr.is_empty() {
        return stdout.to_string();
    }
    format!("{}\n{}", stdout, stderr)
}

/// Build a fallback TestRunResult when output can't be parsed into individual tests.
/// Uses exit code to determine pass/fail.
pub fn fallback_result(
    exit_code: i32,
    adapter_name: &str,
    stdout: &str,
    stderr: &str,
) -> TestRunResult {
    let status = if exit_code == 0 {
        TestStatus::Passed
    } else {
        TestStatus::Failed
    };

    let error = if exit_code != 0 {
        let combined = combined_output(stdout, stderr);
        let message = if combined.is_empty() {
            format!("{} exited with code {}", adapter_name, exit_code)
        } else {
            // Take last few lines as the error message
            let lines: Vec<&str> = combined.lines().collect();
            let start = lines.len().saturating_sub(10);
            lines[start..].join("\n")
        };
        Some(TestError {
            message,
            location: None,
        })
    } else {
        None
    };

    TestRunResult {
        suites: vec![TestSuite {
            name: adapter_name.to_string(),
            tests: vec![TestCase {
                name: format!("{} tests", adapter_name),
                status,
                duration: Duration::ZERO,
                error,
            }],
        }],
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

/// Summary count patterns for different test frameworks.
pub struct SummaryPatterns {
    pub passed: &'static [&'static str],
    pub failed: &'static [&'static str],
    pub skipped: &'static [&'static str],
}

/// Parsed summary counts from a test result line.
#[derive(Debug, Clone, Default)]
pub struct SummaryCounts {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total: usize,
    pub duration: Option<Duration>,
}

impl SummaryCounts {
    pub fn has_any(&self) -> bool {
        self.passed > 0 || self.failed > 0 || self.skipped > 0 || self.total > 0
    }

    pub fn computed_total(&self) -> usize {
        if self.total > 0 {
            self.total
        } else {
            self.passed + self.failed + self.skipped
        }
    }
}

/// Generate synthetic test cases from summary counts.
/// Used when the adapter can only extract totals, not individual test names.
pub fn synthetic_tests_from_counts(counts: &SummaryCounts, suite_name: &str) -> Vec<TestCase> {
    let mut tests = Vec::new();

    for i in 0..counts.passed {
        tests.push(TestCase {
            name: format!("test {} (passed)", i + 1),
            status: TestStatus::Passed,
            duration: Duration::ZERO,
            error: None,
        });
    }

    for i in 0..counts.failed {
        tests.push(TestCase {
            name: format!("test {} (failed)", i + 1),
            status: TestStatus::Failed,
            duration: Duration::ZERO,
            error: Some(TestError {
                message: format!("Test failed in {}", suite_name),
                location: None,
            }),
        });
    }

    for i in 0..counts.skipped {
        tests.push(TestCase {
            name: format!("test {} (skipped)", i + 1),
            status: TestStatus::Skipped,
            duration: Duration::ZERO,
            error: None,
        });
    }

    tests
}

/// Parse a duration string in common formats:
/// "5ms", "1.5s", "0.01 sec", "(5 ms)", "123ms", "1.23s", "0.001s", "5.2 seconds"
pub fn parse_duration_str(s: &str) -> Option<Duration> {
    let s = s.trim().trim_matches(|c| c == '(' || c == ')');

    // Try milliseconds: "123ms", "5 ms"
    if let Some(num) = s
        .strip_suffix("ms")
        .map(|n| n.trim())
        .and_then(|n| n.parse::<f64>().ok())
    {
        return Some(duration_from_secs_safe(num / 1000.0));
    }

    // Try seconds: "1.5s", "0.01 sec", "1.23 seconds"
    let s_stripped = s
        .strip_suffix("seconds")
        .or_else(|| s.strip_suffix("secs"))
        .or_else(|| s.strip_suffix("sec"))
        .or_else(|| s.strip_suffix('s'))
        .map(|n| n.trim());

    if let Some(num) = s_stripped.and_then(|n| n.parse::<f64>().ok()) {
        return Some(duration_from_secs_safe(num));
    }

    // Try minutes: "2m30s", "1.5 min"
    if let Some(num) = s
        .strip_suffix("min")
        .or_else(|| s.strip_suffix('m'))
        .map(|n| n.trim())
        .and_then(|n| n.parse::<f64>().ok())
    {
        return Some(duration_from_secs_safe(num * 60.0));
    }

    None
}

/// Check if a binary is available on PATH. Returns the full path if found.
pub fn check_binary(name: &str) -> Option<String> {
    which::which(name).ok().map(|p| p.display().to_string())
}

/// Check if a binary is available, returning the missing name if not.
pub fn check_runner_binary(name: &str) -> Option<String> {
    if which::which(name).is_err() {
        Some(name.into())
    } else {
        None
    }
}

/// Extract a number from a string that appears before a keyword.
/// E.g., extract_count("3 passed", "passed") => Some(3)
pub fn extract_count(s: &str, keywords: &[&str]) -> Option<usize> {
    for keyword in keywords {
        if let Some(pos) = s.find(keyword) {
            // Look backward from keyword to find the number
            let before = &s[..pos].trim_end();
            // Try to parse the last word as a number
            if let Some(num_str) = before.rsplit_once(|c: char| !c.is_ascii_digit()) {
                if let Ok(n) = num_str.1.parse() {
                    return Some(n);
                }
            } else if let Ok(n) = before.parse() {
                return Some(n);
            }
        }
    }
    None
}

/// Parse counts from a summary line using the given patterns.
pub fn parse_summary_line(line: &str, patterns: &SummaryPatterns) -> SummaryCounts {
    SummaryCounts {
        passed: extract_count(line, patterns.passed).unwrap_or(0),
        failed: extract_count(line, patterns.failed).unwrap_or(0),
        skipped: extract_count(line, patterns.skipped).unwrap_or(0),
        total: 0,
        duration: None,
    }
}

/// Build a detection result with the given parameters.
pub fn make_detection(language: &str, framework: &str, confidence: f32) -> DetectionResult {
    DetectionResult {
        language: language.into(),
        framework: framework.into(),
        confidence,
    }
}

/// Build a Command with the given program and arguments, set to run in the project dir.
pub fn build_test_command(
    program: &str,
    project_dir: &std::path::Path,
    base_args: &[&str],
    extra_args: &[String],
) -> Command {
    let mut cmd = Command::new(program);
    for arg in base_args {
        cmd.arg(arg);
    }
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.current_dir(project_dir);
    cmd
}

/// Escape a string for safe XML output.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Format a Duration as a human-readable string.
pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        return String::new();
    }
    if ms < 1000 {
        format!("{}ms", ms)
    } else if d.as_secs() < 60 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        let mins = d.as_secs() / 60;
        let secs = d.as_secs() % 60;
        format!("{}m{}s", mins, secs)
    }
}

/// Truncate a string to a max length, adding "..." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Extract error context from output lines around a failure.
/// Scans for common failure indicators and returns surrounding context.
pub fn extract_error_context(output: &str, max_lines: usize) -> Option<String> {
    let lines: Vec<&str> = output.lines().collect();
    let error_indicators = [
        "FAILED",
        "FAIL:",
        "Error:",
        "error:",
        "assertion failed",
        "AssertionError",
        "assert_eq!",
        "Expected",
        "expected",
        "panic",
        "PANIC",
        "thread '",
    ];

    for (i, line) in lines.iter().enumerate() {
        for indicator in &error_indicators {
            if line.contains(indicator) {
                let start = i.saturating_sub(2);
                let end = (i + max_lines).min(lines.len());
                return Some(lines[start..end].join("\n"));
            }
        }
    }

    None
}

/// Count lines matching a pattern in the output.
pub fn count_pattern(output: &str, pattern: &str) -> usize {
    output.lines().filter(|l| l.contains(pattern)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_output_both() {
        let result = combined_output("stdout text", "stderr text");
        assert_eq!(result, "stdout text\nstderr text");
    }

    #[test]
    fn combined_output_stdout_only() {
        let result = combined_output("stdout text", "");
        assert_eq!(result, "stdout text");
    }

    #[test]
    fn combined_output_stderr_only() {
        let result = combined_output("", "stderr text");
        assert_eq!(result, "stderr text");
    }

    #[test]
    fn combined_output_both_empty() {
        let result = combined_output("", "");
        assert_eq!(result, "");
    }

    #[test]
    fn combined_output_trims_whitespace() {
        let result = combined_output("  stdout  ", "  stderr  ");
        assert_eq!(result, "stdout\nstderr");
    }

    #[test]
    fn fallback_result_pass() {
        let result = fallback_result(0, "Rust", "all ok", "");
        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
        assert_eq!(result.suites[0].tests[0].error, None);
    }

    #[test]
    fn fallback_result_fail() {
        let result = fallback_result(1, "Python", "", "error happened");
        assert_eq!(result.total_tests(), 1);
        assert!(!result.is_success());
        assert!(result.suites[0].tests[0].error.is_some());
    }

    #[test]
    fn fallback_result_fail_no_output() {
        let result = fallback_result(2, "Go", "", "");
        assert!(
            result.suites[0].tests[0]
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("exited with code 2")
        );
    }

    #[test]
    fn parse_duration_milliseconds() {
        assert_eq!(parse_duration_str("5ms"), Some(Duration::from_millis(5)));
        assert_eq!(
            parse_duration_str("123ms"),
            Some(Duration::from_millis(123))
        );
        assert_eq!(parse_duration_str("0ms"), Some(Duration::from_millis(0)));
    }

    #[test]
    fn parse_duration_milliseconds_with_space() {
        assert_eq!(parse_duration_str("5 ms"), Some(Duration::from_millis(5)));
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(
            parse_duration_str("1.5s"),
            Some(Duration::from_secs_f64(1.5))
        );
        assert_eq!(
            parse_duration_str("0.01s"),
            Some(Duration::from_secs_f64(0.01))
        );
    }

    #[test]
    fn parse_duration_seconds_long_form() {
        assert_eq!(
            parse_duration_str("2.5 sec"),
            Some(Duration::from_secs_f64(2.5))
        );
        assert_eq!(
            parse_duration_str("1 seconds"),
            Some(Duration::from_secs_f64(1.0))
        );
    }

    #[test]
    fn parse_duration_with_parens() {
        assert_eq!(parse_duration_str("(5ms)"), Some(Duration::from_millis(5)));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration_str("1.5min"), Some(Duration::from_secs(90)));
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_duration_str("hello"), None);
        assert_eq!(parse_duration_str(""), None);
        assert_eq!(parse_duration_str("abc ms"), None);
    }

    #[test]
    fn check_binary_exists() {
        // "sh" should exist on any Unix system
        assert!(check_binary("sh").is_some());
    }

    #[test]
    fn check_binary_not_found() {
        assert!(check_binary("definitely_not_a_real_binary_12345").is_none());
    }

    #[test]
    fn check_runner_binary_exists() {
        assert!(check_runner_binary("sh").is_none()); // None = no missing runner
    }

    #[test]
    fn check_runner_binary_missing() {
        let result = check_runner_binary("nonexistent_runner_xyz");
        assert_eq!(result, Some("nonexistent_runner_xyz".into()));
    }

    #[test]
    fn extract_count_simple() {
        assert_eq!(extract_count("3 passed", &["passed"]), Some(3));
        assert_eq!(extract_count("12 failed", &["failed"]), Some(12));
        assert_eq!(extract_count("0 skipped", &["skipped"]), Some(0));
    }

    #[test]
    fn extract_count_multiple_keywords() {
        assert_eq!(extract_count("5 passed", &["passed", "ok"]), Some(5));
        assert_eq!(extract_count("5 ok", &["passed", "ok"]), Some(5));
    }

    #[test]
    fn extract_count_in_summary() {
        let line = "3 passed, 1 failed, 2 skipped";
        assert_eq!(extract_count(line, &["passed"]), Some(3));
        assert_eq!(extract_count(line, &["failed"]), Some(1));
        assert_eq!(extract_count(line, &["skipped"]), Some(2));
    }

    #[test]
    fn extract_count_not_found() {
        assert_eq!(extract_count("all fine", &["passed"]), None);
    }

    #[test]
    fn parse_summary_line_full() {
        let patterns = SummaryPatterns {
            passed: &["passed"],
            failed: &["failed"],
            skipped: &["skipped"],
        };
        let counts = parse_summary_line("3 passed, 1 failed, 2 skipped", &patterns);
        assert_eq!(counts.passed, 3);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.skipped, 2);
    }

    #[test]
    fn summary_counts_has_any() {
        let empty = SummaryCounts::default();
        assert!(!empty.has_any());

        let with_passed = SummaryCounts {
            passed: 1,
            ..Default::default()
        };
        assert!(with_passed.has_any());
    }

    #[test]
    fn summary_counts_computed_total() {
        let counts = SummaryCounts {
            passed: 3,
            failed: 1,
            skipped: 2,
            total: 0,
            duration: None,
        };
        assert_eq!(counts.computed_total(), 6);

        let with_total = SummaryCounts {
            total: 10,
            ..Default::default()
        };
        assert_eq!(with_total.computed_total(), 10);
    }

    #[test]
    fn synthetic_tests_from_counts_all_types() {
        let counts = SummaryCounts {
            passed: 2,
            failed: 1,
            skipped: 1,
            total: 4,
            duration: None,
        };
        let tests = synthetic_tests_from_counts(&counts, "tests");
        assert_eq!(tests.len(), 4);
        assert_eq!(
            tests
                .iter()
                .filter(|t| t.status == TestStatus::Passed)
                .count(),
            2
        );
        assert_eq!(
            tests
                .iter()
                .filter(|t| t.status == TestStatus::Failed)
                .count(),
            1
        );
        assert_eq!(
            tests
                .iter()
                .filter(|t| t.status == TestStatus::Skipped)
                .count(),
            1
        );
    }

    #[test]
    fn synthetic_tests_empty_counts() {
        let counts = SummaryCounts::default();
        let tests = synthetic_tests_from_counts(&counts, "tests");
        assert!(tests.is_empty());
    }

    #[test]
    fn make_detection_helper() {
        let det = make_detection("Rust", "cargo test", 0.95);
        assert_eq!(det.language, "Rust");
        assert_eq!(det.framework, "cargo test");
        assert!((det.confidence - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn build_test_command_basic() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = build_test_command("echo", dir.path(), &["hello"], &[]);
        let program = cmd.get_program().to_string_lossy();
        assert_eq!(program, "echo");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["hello"]);
    }

    #[test]
    fn build_test_command_with_extra_args() {
        let dir = tempfile::tempdir().unwrap();
        let extra = vec!["--verbose".to_string(), "--color".to_string()];
        let cmd = build_test_command("cargo", dir.path(), &["test"], &extra);
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["test", "--verbose", "--color"]);
    }

    #[test]
    fn xml_escape_special_chars() {
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn xml_escape_no_special() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Duration::ZERO), "");
    }

    #[test]
    fn format_duration_milliseconds() {
        assert_eq!(format_duration(Duration::from_millis(42)), "42ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.50s");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.00s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1m30s");
        assert_eq!(format_duration(Duration::from_secs(120)), "2m0s");
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world foo bar", 10), "hello w...");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn extract_error_context_found() {
        let output = "line 1\nline 2\nFAILED test_foo\nline 4\nline 5";
        let ctx = extract_error_context(output, 3);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("FAILED test_foo"));
    }

    #[test]
    fn extract_error_context_not_found() {
        let output = "all tests passed\neverything is fine";
        assert!(extract_error_context(output, 3).is_none());
    }

    #[test]
    fn extract_error_context_at_start() {
        let output = "FAILED immediately\nmore info\neven more";
        let ctx = extract_error_context(output, 3).unwrap();
        assert!(ctx.contains("FAILED immediately"));
    }

    #[test]
    fn count_pattern_basic() {
        let output = "ok test_1\nFAIL test_2\nok test_3\nFAIL test_4";
        assert_eq!(count_pattern(output, "ok"), 2);
        assert_eq!(count_pattern(output, "FAIL"), 2);
    }

    #[test]
    fn count_pattern_none() {
        assert_eq!(count_pattern("hello world", "FAIL"), 0);
    }
}
