use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::{combined_output, duration_from_secs_safe, truncate};
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus,
    TestSuite,
};

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

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, project_dir.join("build.zig.zon").exists())
            .signal(0.10, project_dir.join("src").is_dir())
            .signal(0.15, which::which("zig").is_ok())
            .finish();

        Some(DetectionResult {
            language: "Zig".into(),
            framework: "zig test".into(),
            confidence,
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

    fn filter_args(&self, pattern: &str) -> Vec<String> {
        // zig test uses --test-filter
        vec!["--test-filter".to_string(), pattern.to_string()]
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = combined_output(stdout, stderr);

        let mut suites = parse_zig_output(&combined, exit_code);

        // Enrich with error details from zig test output
        let failures = parse_zig_failures(&combined);
        if !failures.is_empty() {
            enrich_with_errors(&mut suites, &failures);
        }

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
                return Some(duration_from_secs_safe(secs));
            }
        }
    }
    None
}

/// A parsed failure from Zig test output.
#[derive(Debug, Clone)]
struct ZigTestFailure {
    /// Test name
    test_name: String,
    /// Error message (panic message or assertion error)
    message: String,
    /// Source location if available
    location: Option<String>,
}

/// Parse Zig test failure details.
///
/// Zig test failures produce output like:
/// ```text
/// Test [3/3] test.edge case... FAIL
/// /home/user/src/main.zig:42:5: 0x1234abcd in test.edge case (test)
///     unreachable
/// /usr/lib/zig/std/debug.zig:100:0: ...
/// error: test.edge case... FAIL
/// ```
///
/// Or panic messages:
/// ```text
/// Test [2/3] test.divide... thread 12345 panic:
/// integer overflow
/// /home/user/src/math.zig:10:12: 0x1234 in test.divide (test)
/// ```
///
/// Or compile errors:
/// ```text
/// src/main.zig:42:5: error: expected type 'u32', found 'i32'
/// ```
fn parse_zig_failures(output: &str) -> Vec<ZigTestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // "Test [N/M] test.name... FAIL"
        if trimmed.starts_with("Test [") && trimmed.ends_with("FAIL") {
            let test_name = extract_zig_test_name(trimmed);

            // Collect following error lines
            let mut error_lines = Vec::new();
            let mut location = None;
            i += 1;

            while i < lines.len() {
                let line = lines[i].trim();

                // Stop at next test or summary
                if line.starts_with("Test [")
                    || line.contains("passed")
                    || line.is_empty() && error_lines.len() > 3
                {
                    break;
                }

                if !line.is_empty() {
                    // Extract source location
                    if location.is_none() && is_zig_source_location(line) {
                        location = Some(extract_zig_location(line));
                    }

                    error_lines.push(line.to_string());
                }

                i += 1;
            }

            let message = if error_lines.is_empty() {
                "Test failed".to_string()
            } else {
                // Use the most informative line as the message
                find_zig_error_message(&error_lines)
            };

            failures.push(ZigTestFailure {
                test_name,
                message: truncate(&message, 500),
                location,
            });
            continue;
        }

        // Compile error: "src/main.zig:42:5: error: ..."
        if is_zig_compile_error(trimmed) {
            let (location, message) = parse_zig_compile_error(trimmed);
            failures.push(ZigTestFailure {
                test_name: "compile_error".into(),
                message: truncate(&message, 500),
                location: Some(location),
            });
        }

        // Panic: "panic: ..." or "thread N panic:"
        if trimmed.contains("panic:") && !trimmed.starts_with("Test [") {
            let message = trimmed
                .split("panic:")
                .nth(1)
                .unwrap_or(trimmed)
                .trim()
                .to_string();

            // Look ahead for the source location
            let mut location = None;
            let mut j = i + 1;
            while j < lines.len() && j < i + 5 {
                if is_zig_source_location(lines[j].trim()) {
                    location = Some(extract_zig_location(lines[j].trim()));
                    break;
                }
                j += 1;
            }

            failures.push(ZigTestFailure {
                test_name: "panic".into(),
                message: truncate(&message, 500),
                location,
            });
        }

        i += 1;
    }

    failures
}

/// Extract test name from "Test [N/M] test.name... FAIL"
fn extract_zig_test_name(line: &str) -> String {
    if let Some(bracket_end) = line.find("] ") {
        let after = &line[bracket_end + 2..];
        after
            .rfind("...")
            .map(|i| after[..i].trim())
            .unwrap_or(after)
            .to_string()
    } else {
        "unknown".into()
    }
}

/// Check if a line is a Zig source location.
/// "/path/to/file.zig:42:5: ..."
fn is_zig_source_location(line: &str) -> bool {
    line.contains(".zig:") && {
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        parts.len() >= 3 && parts[1].chars().all(|c| c.is_ascii_digit())
    }
}

/// Extract location from a Zig source line.
/// "/path/to/file.zig:42:5: 0x1234 in test.name" -> "/path/to/file.zig:42:5"
fn extract_zig_location(line: &str) -> String {
    // Find the first two colons after .zig
    if let Some(zig_idx) = line.find(".zig:") {
        let after_zig = &line[zig_idx + 5..]; // after ".zig:"
        // Find the line number (digits)
        let num_end = after_zig
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_zig.len());
        if num_end > 0 {
            let after_line = &after_zig[num_end..];
            if let Some(col_str) = after_line.strip_prefix(':') {
                // Also include column number
                let col_end = col_str
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(col_str.len());
                if col_end > 0 {
                    return line[..zig_idx + 5 + num_end + 1 + col_end].to_string();
                }
            }
            return line[..zig_idx + 5 + num_end].to_string();
        }
    }
    line.to_string()
}

/// Find the most informative error message from a list of lines.
fn find_zig_error_message(lines: &[String]) -> String {
    // Prefer lines with "error:", "panic:", "unreachable", "assertion"
    for line in lines {
        let lower = line.to_lowercase();
        if lower.contains("error:")
            || lower.contains("panic:")
            || lower.contains("unreachable")
            || lower.contains("assertion")
            || lower.contains("expected")
        {
            return line.clone();
        }
    }
    // Fall back to first non-empty, non-location line
    for line in lines {
        if !is_zig_source_location(line) && !line.trim().is_empty() {
            return line.clone();
        }
    }
    lines
        .first()
        .cloned()
        .unwrap_or_else(|| "Test failed".to_string())
}

/// Check if a line is a Zig compile error.
fn is_zig_compile_error(line: &str) -> bool {
    line.contains(".zig:") && line.contains(": error:")
}

/// Parse a Zig compile error line.
/// "src/main.zig:42:5: error: expected type 'u32', found 'i32'"
fn parse_zig_compile_error(line: &str) -> (String, String) {
    if let Some(error_idx) = line.find(": error:") {
        let location = line[..error_idx].to_string();
        let message = line[error_idx + 8..].trim().to_string();
        (location, message)
    } else {
        (line.to_string(), "compile error".to_string())
    }
}

/// Enrich test cases with failure details.
fn enrich_with_errors(suites: &mut [TestSuite], failures: &[ZigTestFailure]) {
    for suite in suites.iter_mut() {
        for test in suite.tests.iter_mut() {
            if test.status != TestStatus::Failed || test.error.is_some() {
                continue;
            }
            if let Some(failure) = find_matching_zig_failure(&test.name, failures) {
                test.error = Some(TestError {
                    message: failure.message.clone(),
                    location: failure.location.clone(),
                });
            }
        }
    }
}

/// Find a matching failure for a test name.
fn find_matching_zig_failure<'a>(
    test_name: &str,
    failures: &'a [ZigTestFailure],
) -> Option<&'a ZigTestFailure> {
    for failure in failures {
        if failure.test_name == test_name {
            return Some(failure);
        }
        // Partial match
        if test_name.contains(&failure.test_name) || failure.test_name.contains(test_name) {
            return Some(failure);
        }
    }
    if failures.len() == 1 {
        return Some(&failures[0]);
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

    #[test]
    fn parse_zig_failure_with_error_details() {
        let stdout = r#"
Test [1/2] test.basic... OK
Test [2/2] test.edge case... FAIL
/home/user/src/main.zig:42:5: 0x1234abcd in test.edge case (test)
    unreachable
/usr/lib/zig/std/debug.zig:100:0: in std.debug.panicImpl

1 passed; 1 failed.
"#;
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_failed(), 1);
        let failed = result.suites[0]
            .tests
            .iter()
            .find(|t| t.status == TestStatus::Failed)
            .unwrap();
        assert!(failed.error.is_some());
        let err = failed.error.as_ref().unwrap();
        assert!(err.location.is_some());
    }

    #[test]
    fn parse_zig_failures_basic() {
        let output = r#"
Test [1/1] test.divide... FAIL
/home/user/src/math.zig:10:12: 0x1234 in test.divide (test)
    integer overflow
"#;
        let failures = parse_zig_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].test_name, "test.divide");
        assert!(failures[0].location.is_some());
    }

    #[test]
    fn parse_zig_compile_error_test() {
        let output = "src/main.zig:42:5: error: expected type 'u32', found 'i32'\n";
        let failures = parse_zig_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].test_name, "compile_error");
        assert!(failures[0].message.contains("expected type"));
    }

    #[test]
    fn parse_zig_panic_test() {
        let output = r#"
thread 12345 panic: integer overflow
/home/user/src/math.zig:10:12: in test_fn
"#;
        let failures = parse_zig_failures(output);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message.contains("integer overflow"));
    }

    #[test]
    fn extract_zig_test_name_test() {
        assert_eq!(
            extract_zig_test_name("Test [1/3] test.basic add... OK"),
            "test.basic add"
        );
        assert_eq!(
            extract_zig_test_name("Test [2/3] test.edge... FAIL"),
            "test.edge"
        );
    }

    #[test]
    fn is_zig_source_location_test() {
        assert!(is_zig_source_location(
            "/home/user/src/main.zig:42:5: in test"
        ));
        assert!(is_zig_source_location("src/math.zig:10:12: error"));
        assert!(!is_zig_source_location("not a location"));
        assert!(!is_zig_source_location(
            "some text.zig without colon numbers"
        ));
    }

    #[test]
    fn extract_zig_location_test() {
        assert_eq!(
            extract_zig_location("/home/user/src/main.zig:42:5: 0x1234 in test"),
            "/home/user/src/main.zig:42:5"
        );
        assert_eq!(
            extract_zig_location("src/math.zig:10:12: error: boom"),
            "src/math.zig:10:12"
        );
    }

    #[test]
    fn find_zig_error_message_test() {
        let lines = vec![
            "/src/main.zig:42:5: 0x1234".into(),
            "unreachable".into(),
            "/lib/debug.zig:100:0: in something".into(),
        ];
        let msg = find_zig_error_message(&lines);
        assert_eq!(msg, "unreachable");
    }

    #[test]
    fn find_zig_error_message_with_error() {
        let lines = vec![
            "error: expected type 'u32'".into(),
            "some other line".into(),
        ];
        let msg = find_zig_error_message(&lines);
        assert!(msg.contains("error:"));
    }

    #[test]
    fn is_zig_compile_error_test() {
        assert!(is_zig_compile_error(
            "src/main.zig:42:5: error: expected type"
        ));
        assert!(!is_zig_compile_error("not a compile error"));
    }

    #[test]
    fn parse_zig_compile_error_line() {
        let (loc, msg) =
            parse_zig_compile_error("src/main.zig:42:5: error: expected type 'u32', found 'i32'");
        assert_eq!(loc, "src/main.zig:42:5");
        assert_eq!(msg, "expected type 'u32', found 'i32'");
    }

    #[test]
    fn truncate_test() {
        assert_eq!(truncate("short", 100), "short");
        let long = "z".repeat(600);
        let truncated = truncate(&long, 500);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn enrich_with_errors_test() {
        let mut suites = vec![TestSuite {
            name: "tests".into(),
            tests: vec![
                TestCase {
                    name: "test.add".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "test.edge".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
            ],
        }];
        let failures = vec![ZigTestFailure {
            test_name: "test.edge".into(),
            message: "unreachable".into(),
            location: Some("/src/main.zig:42:5".into()),
        }];
        enrich_with_errors(&mut suites, &failures);
        assert!(suites[0].tests[0].error.is_none());
        let err = suites[0].tests[1].error.as_ref().unwrap();
        assert_eq!(err.message, "unreachable");
        assert!(err.location.as_ref().unwrap().contains("main.zig:42:5"));
    }

    #[test]
    fn parse_zig_test_integration() {
        let stdout = r#"
Test [1/3] test.basic add... OK
Test [2/3] test.advanced... OK
Test [3/3] test.edge case... FAIL
/src/main.zig:42:5: 0x1234 in test.edge case
    error: assertion failed
2 passed; 1 failed.
"#;
        let adapter = ZigAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        let failed = result.suites[0]
            .tests
            .iter()
            .find(|t| t.status == TestStatus::Failed)
            .unwrap();
        assert!(failed.error.is_some());
    }
}
