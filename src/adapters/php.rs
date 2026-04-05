use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::{combined_output, duration_from_secs_safe, truncate};
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus,
    TestSuite,
};

pub struct PhpAdapter;

impl Default for PhpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpAdapter {
    pub fn new() -> Self {
        Self
    }

    fn has_phpunit_config(project_dir: &Path) -> bool {
        project_dir.join("phpunit.xml").exists() || project_dir.join("phpunit.xml.dist").exists()
    }

    fn has_vendor_phpunit(project_dir: &Path) -> bool {
        project_dir.join("vendor/bin/phpunit").exists()
    }

    fn has_composer_phpunit(project_dir: &Path) -> bool {
        let composer = project_dir.join("composer.json");
        if composer.exists()
            && let Ok(content) = std::fs::read_to_string(&composer)
        {
            return content.contains("phpunit");
        }
        false
    }
}

impl TestAdapter for PhpAdapter {
    fn name(&self) -> &str {
        "PHP"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("php").is_err() {
            return Some("php not found. Install PHP.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        if !Self::has_phpunit_config(project_dir) && !Self::has_composer_phpunit(project_dir) {
            return None;
        }

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, Self::has_phpunit_config(project_dir))
            .signal(0.10, Self::has_vendor_phpunit(project_dir))
            .signal(
                0.10,
                project_dir.join("tests").is_dir() || project_dir.join("test").is_dir(),
            )
            .signal(0.07, which::which("php").is_ok())
            .finish();

        Some(DetectionResult {
            language: "PHP".into(),
            framework: "PHPUnit".into(),
            confidence,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd;

        if Self::has_vendor_phpunit(project_dir) {
            cmd = Command::new("./vendor/bin/phpunit");
        } else {
            cmd = Command::new("phpunit");
        }

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn filter_args(&self, pattern: &str) -> Vec<String> {
        vec!["--filter".to_string(), pattern.to_string()]
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = combined_output(stdout, stderr);

        // Try verbose --testdox output first, then standard summary
        let mut suites = parse_testdox_output(&combined);
        if suites.is_empty() || suites.iter().all(|s| s.tests.is_empty()) {
            suites = parse_phpunit_output(&combined, exit_code);
        }

        // Enrich with failure details
        let failures = parse_phpunit_failures(&combined);
        if !failures.is_empty() {
            enrich_with_errors(&mut suites, &failures);
        }

        let duration = parse_phpunit_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse PHPUnit output.
///
/// Format:
/// ```text
/// PHPUnit 10.5.0 by Sebastian Bergmann and contributors.
///
/// ..F.S                                                               5 / 5 (100%)
///
/// Time: 00:00.012, Memory: 8.00 MB
///
/// There was 1 failure:
///
/// 1) Tests\CalculatorTest::testDivision
/// Failed asserting that 3 matches expected 4.
///
/// FAILURES!
/// Tests: 5, Assertions: 5, Failures: 1, Skipped: 1.
/// ```
fn parse_phpunit_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary: "Tests: 5, Assertions: 5, Failures: 1, Skipped: 1."
        // Or success: "OK (5 tests, 5 assertions)"
        if trimmed.starts_with("Tests:") && trimmed.contains("Assertions:") {
            let mut total = 0usize;
            let mut failures = 0usize;
            let mut errors = 0usize;
            let mut skipped = 0usize;

            for part in trimmed.split(',') {
                let part = part.trim().trim_end_matches('.');
                if let Some(rest) = part.strip_prefix("Tests:") {
                    total = rest.trim().parse().unwrap_or(0);
                } else if let Some(rest) = part.strip_prefix("Failures:") {
                    failures = rest.trim().parse().unwrap_or(0);
                } else if let Some(rest) = part.strip_prefix("Errors:") {
                    errors = rest.trim().parse().unwrap_or(0);
                } else if let Some(rest) = part.strip_prefix("Skipped:") {
                    skipped = rest.trim().parse().unwrap_or(0);
                } else if let Some(rest) = part.strip_prefix("Incomplete:") {
                    skipped += rest.trim().parse::<usize>().unwrap_or(0);
                }
            }

            let failed = failures + errors;
            let passed = total.saturating_sub(failed + skipped);

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
            break;
        }

        // "OK (5 tests, 5 assertions)"
        if trimmed.starts_with("OK (") && trimmed.contains("test") {
            let inner = trimmed
                .strip_prefix("OK (")
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or("");
            for part in inner.split(',') {
                let part = part.trim();
                let words: Vec<&str> = part.split_whitespace().collect();
                if words.len() >= 2 && words[1].starts_with("test") {
                    let count: usize = words[0].parse().unwrap_or(0);
                    for i in 0..count {
                        tests.push(TestCase {
                            name: format!("test_{}", i + 1),
                            status: TestStatus::Passed,
                            duration: Duration::from_millis(0),
                            error: None,
                        });
                    }
                    break;
                }
            }
            break;
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

fn parse_phpunit_duration(output: &str) -> Option<Duration> {
    // "Time: 00:00.012, Memory: 8.00 MB"
    for line in output.lines() {
        if line.contains("Time:")
            && line.contains("Memory:")
            && let Some(idx) = line.find("Time:")
        {
            let after = &line[idx + 5..];
            let time_str = after.split(',').next()?.trim();
            // Format: "00:00.012" (MM:SS.mmm)
            if let Some(colon_idx) = time_str.find(':') {
                let mins: f64 = time_str[..colon_idx].parse().unwrap_or(0.0);
                let secs: f64 = time_str[colon_idx + 1..].parse().unwrap_or(0.0);
                return Some(duration_from_secs_safe(mins * 60.0 + secs));
            }
        }
    }
    None
}

/// Parse PHPUnit --testdox verbose output.
///
/// Format:
/// ```text
/// Calculator (Tests\Calculator)
///  ✔ Can add two numbers
///  ✔ Can subtract two numbers
///  ✘ Can divide by zero
///  ⚬ Can multiply large numbers
/// ```
fn parse_testdox_output(output: &str) -> Vec<TestSuite> {
    let mut suites: Vec<TestSuite> = Vec::new();
    let mut current_suite = String::new();
    let mut current_tests: Vec<TestCase> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Suite header: "ClassName (Namespace\ClassName)" or just "ClassName"
        if is_testdox_suite_header(trimmed) {
            if !current_suite.is_empty() && !current_tests.is_empty() {
                suites.push(TestSuite {
                    name: current_suite.clone(),
                    tests: std::mem::take(&mut current_tests),
                });
            }
            // Extract suite name: use the part before " (" if present
            current_suite = trimmed
                .find(" (")
                .map(|i| trimmed[..i].to_string())
                .unwrap_or_else(|| trimmed.to_string());
            continue;
        }

        // Test line: " ✔ Can add two numbers" or " ✘ Can divide" or " ⚬ Skipped test"
        if let Some(test) = parse_testdox_test_line(trimmed) {
            current_tests.push(test);
        }
    }

    // Flush last suite
    if !current_suite.is_empty() && !current_tests.is_empty() {
        suites.push(TestSuite {
            name: current_suite,
            tests: current_tests,
        });
    }

    suites
}

/// Check if a line is a testdox suite header.
/// Suite headers are non-empty lines that don't start with test markers
/// and typically contain a class name.
fn is_testdox_suite_header(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    // Must not start with test markers
    if line.starts_with('✔')
        || line.starts_with('✘')
        || line.starts_with('⚬')
        || line.starts_with('✓')
        || line.starts_with('✗')
        || line.starts_with('×')
        || line.starts_with('-')
    {
        return false;
    }
    // Must not be a known non-header line
    if line.starts_with("PHPUnit")
        || line.starts_with("Time:")
        || line.starts_with("OK ")
        || line.starts_with("Tests:")
        || line.starts_with("FAILURES!")
        || line.starts_with("ERRORS!")
        || line.starts_with("There ")
        || line.contains("test") && line.contains("assertion")
    {
        return false;
    }
    // Should start with uppercase letter (class name)
    line.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Parse a single testdox test line.
fn parse_testdox_test_line(line: &str) -> Option<TestCase> {
    // " ✔ Can add two numbers" or "✔ Can add"
    let (status, rest) = if let Some(r) = strip_testdox_marker(line, &['✔', '✓']) {
        (TestStatus::Passed, r)
    } else if let Some(r) = strip_testdox_marker(line, &['✘', '✗', '×']) {
        (TestStatus::Failed, r)
    } else if let Some(r) = strip_testdox_marker(line, &['⚬', '○', '-']) {
        (TestStatus::Skipped, r)
    } else {
        return None;
    };

    let name = rest.trim().to_string();
    if name.is_empty() {
        return None;
    }

    // Try to extract inline duration: "Can add two numbers (0.123s)"
    let (clean_name, duration) = extract_testdox_duration(&name);

    Some(TestCase {
        name: clean_name,
        status,
        duration,
        error: None,
    })
}

/// Strip a testdox marker character from the beginning of a line.
fn strip_testdox_marker<'a>(line: &'a str, markers: &[char]) -> Option<&'a str> {
    for &marker in markers {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest.trim_start());
        }
    }
    None
}

/// Extract optional duration from a testdox test name.
/// "Can add two numbers (0.123s)" -> ("Can add two numbers", Duration)
fn extract_testdox_duration(name: &str) -> (String, Duration) {
    if let Some(paren_start) = name.rfind('(') {
        let inside = &name[paren_start + 1..name.len().saturating_sub(1)];
        let inside = inside.trim();
        if (inside.ends_with('s') || inside.ends_with("ms"))
            && let Some(dur) = parse_testdox_duration_str(inside)
        {
            let clean = name[..paren_start].trim().to_string();
            return (clean, dur);
        }
    }
    (name.to_string(), Duration::from_millis(0))
}

/// Parse a testdox duration string: "0.123s", "123ms"
fn parse_testdox_duration_str(s: &str) -> Option<Duration> {
    if let Some(rest) = s.strip_suffix("ms") {
        let val: f64 = rest.trim().parse().ok()?;
        Some(duration_from_secs_safe(val / 1000.0))
    } else if let Some(rest) = s.strip_suffix('s') {
        let val: f64 = rest.trim().parse().ok()?;
        Some(duration_from_secs_safe(val))
    } else {
        None
    }
}

/// A parsed failure from PHPUnit output.
#[derive(Debug, Clone)]
struct PhpUnitFailure {
    /// The fully-qualified test method name (e.g., "Tests\CalculatorTest::testDivision")
    test_method: String,
    /// The error/assertion message
    message: String,
    /// The file location if available
    location: Option<String>,
}

/// Parse PHPUnit failure blocks.
///
/// Format:
/// ```text
/// There was 1 failure:
///
/// 1) Tests\CalculatorTest::testDivision
/// Failed asserting that 3 matches expected 4.
///
/// /path/to/tests/CalculatorTest.php:42
///
/// --
///
/// There were 2 errors:
///
/// 1) Tests\AppTest::testBroken
/// Error: Call to undefined function
///
/// /path/to/tests/AppTest.php:15
/// ```
fn parse_phpunit_failures(output: &str) -> Vec<PhpUnitFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Detect failure header: "1) Tests\CalculatorTest::testDivision"
        if is_phpunit_failure_header(trimmed) {
            let test_method = trimmed
                .find(") ")
                .map(|idx| trimmed[idx + 2..].trim().to_string())
                .unwrap_or_default();

            // Collect message lines until we hit an empty line or another failure header
            let mut message_lines = Vec::new();
            let mut location = None;
            i += 1;

            while i < lines.len() {
                let line = lines[i].trim();

                // Empty line might precede location or end of block
                if line.is_empty() {
                    i += 1;
                    // Check if next line is a file location
                    if i < lines.len() && is_php_file_location(lines[i].trim()) {
                        location = Some(lines[i].trim().to_string());
                        i += 1;
                    }
                    break;
                }

                // File location line
                if is_php_file_location(line) {
                    location = Some(line.to_string());
                    i += 1;
                    break;
                }

                // Next failure header
                if is_phpunit_failure_header(line) {
                    break;
                }

                message_lines.push(line.to_string());
                i += 1;
            }

            if !test_method.is_empty() {
                failures.push(PhpUnitFailure {
                    test_method,
                    message: truncate(&message_lines.join("\n"), 500),
                    location,
                });
            }
            continue;
        }

        i += 1;
    }

    failures
}

/// Check if a line is a PHPUnit failure header: "1) Tests\CalculatorTest::testDivision"
fn is_phpunit_failure_header(line: &str) -> bool {
    if line.len() < 3 {
        return false;
    }
    // Must start with a digit, then ")"
    let mut chars = line.chars();
    let first = chars.next().unwrap_or(' ');
    if !first.is_ascii_digit() {
        return false;
    }
    // Find the ") " pattern
    line.contains(") ") && line.find(") ").is_some_and(|idx| idx <= 5)
}

/// Check if a line looks like a PHP file location: "/path/to/file.php:42"
fn is_php_file_location(line: &str) -> bool {
    (line.contains(".php:") || line.contains(".php("))
        && (line.starts_with('/') || line.starts_with('\\') || line.contains(":\\"))
}

/// Enrich test cases with failure details.
fn enrich_with_errors(suites: &mut [TestSuite], failures: &[PhpUnitFailure]) {
    for suite in suites.iter_mut() {
        for test in suite.tests.iter_mut() {
            if test.status != TestStatus::Failed || test.error.is_some() {
                continue;
            }
            // Try to match by test name
            if let Some(failure) = find_matching_failure(&test.name, failures) {
                test.error = Some(TestError {
                    message: failure.message.clone(),
                    location: failure.location.clone(),
                });
            }
        }
    }
}

/// Find a matching failure for a test name.
/// PHPUnit failure headers use "Namespace\Class::method" format.
/// Test names from testdox are human-readable, from summary they're synthetic.
fn find_matching_failure<'a>(
    test_name: &str,
    failures: &'a [PhpUnitFailure],
) -> Option<&'a PhpUnitFailure> {
    // Direct match on method name
    for failure in failures {
        // Extract just the method name from "Namespace\Class::method"
        let method = failure
            .test_method
            .rsplit("::")
            .next()
            .unwrap_or(&failure.test_method);
        if test_name.eq_ignore_ascii_case(method) {
            return Some(failure);
        }
        // testdox format converts "testCanAddNumbers" to "Can add numbers"
        if testdox_matches(test_name, method) {
            return Some(failure);
        }
    }
    // If there's exactly one failure and one failed test, match them
    if failures.len() == 1 {
        return Some(&failures[0]);
    }
    None
}

/// Check if a testdox-style name matches a test method name.
/// "Can add two numbers" should match "testCanAddTwoNumbers"
fn testdox_matches(testdox_name: &str, method_name: &str) -> bool {
    // Strip "test" prefix and convert camelCase to words
    let method = method_name.strip_prefix("test").unwrap_or(method_name);
    let method_words = camel_case_to_words(method);
    let testdox_lower = testdox_name.to_lowercase();
    method_words.to_lowercase() == testdox_lower
}

/// Convert camelCase to space-separated words.
/// "CanAddTwoNumbers" -> "can add two numbers"
fn camel_case_to_words(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            result.push(' ');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_phpunit_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("phpunit.xml"),
            "<phpunit><testsuites/></phpunit>",
        )
        .unwrap();
        let adapter = PhpAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "PHP");
        assert_eq!(det.framework, "PHPUnit");
    }

    #[test]
    fn detect_phpunit_dist() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("phpunit.xml.dist"), "<phpunit/>").unwrap();
        let adapter = PhpAdapter::new();
        assert!(adapter.detect(dir.path()).is_some());
    }

    #[test]
    fn detect_composer_phpunit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"require-dev":{"phpunit/phpunit":"^10"}}"#,
        )
        .unwrap();
        let adapter = PhpAdapter::new();
        assert!(adapter.detect(dir.path()).is_some());
    }

    #[test]
    fn detect_no_php() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = PhpAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_phpunit_failures_summary() {
        let stdout = r#"
PHPUnit 10.5.0 by Sebastian Bergmann and contributors.

..F.S                                                               5 / 5 (100%)

Time: 00:00.012, Memory: 8.00 MB

FAILURES!
Tests: 5, Assertions: 5, Failures: 1, Skipped: 1.
"#;
        let adapter = PhpAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 3);
        assert_eq!(result.total_failed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_phpunit_all_pass() {
        let stdout = r#"
PHPUnit 10.5.0

.....                                                               5 / 5 (100%)

Time: 00:00.005, Memory: 8.00 MB

OK (5 tests, 5 assertions)
"#;
        let adapter = PhpAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 5);
        assert!(result.is_success());
    }

    #[test]
    fn parse_phpunit_with_errors() {
        let stdout = "Tests: 3, Assertions: 3, Errors: 1.\n";
        let adapter = PhpAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_phpunit_empty_output() {
        let adapter = PhpAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_phpunit_duration_test() {
        assert_eq!(
            parse_phpunit_duration("Time: 00:01.500, Memory: 8.00 MB"),
            Some(Duration::from_millis(1500))
        );
    }

    #[test]
    fn parse_testdox_basic() {
        let output = r#"
Calculator (Tests\Calculator)
 ✔ Can add two numbers
 ✔ Can subtract two numbers
 ✘ Can divide by zero
"#;
        let suites = parse_testdox_output(output);
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].name, "Calculator");
        assert_eq!(suites[0].tests.len(), 3);
        assert_eq!(suites[0].tests[0].name, "Can add two numbers");
        assert_eq!(suites[0].tests[0].status, TestStatus::Passed);
        assert_eq!(suites[0].tests[2].status, TestStatus::Failed);
    }

    #[test]
    fn parse_testdox_multiple_suites() {
        let output = r#"
Calculator (Tests\Calculator)
 ✔ Can add
 ✔ Can subtract

StringHelper (Tests\StringHelper)
 ✔ Can uppercase
 ✘ Can reverse
 ⚬ Can truncate
"#;
        let suites = parse_testdox_output(output);
        assert_eq!(suites.len(), 2);
        assert_eq!(suites[0].name, "Calculator");
        assert_eq!(suites[0].tests.len(), 2);
        assert_eq!(suites[1].name, "StringHelper");
        assert_eq!(suites[1].tests.len(), 3);
        assert_eq!(suites[1].tests[2].status, TestStatus::Skipped);
    }

    #[test]
    fn parse_testdox_with_duration() {
        let output = r#"
Calculator (Tests\Calculator)
 ✔ Can add two numbers (0.005s)
"#;
        let suites = parse_testdox_output(output);
        assert_eq!(suites[0].tests[0].name, "Can add two numbers");
        assert!(suites[0].tests[0].duration.as_micros() > 0);
    }

    #[test]
    fn parse_testdox_empty_output() {
        let suites = parse_testdox_output("");
        assert!(suites.is_empty());
    }

    #[test]
    fn is_testdox_suite_header_various() {
        assert!(is_testdox_suite_header("Calculator (Tests\\Calculator)"));
        assert!(is_testdox_suite_header("MyClass"));
        assert!(!is_testdox_suite_header(""));
        assert!(!is_testdox_suite_header("✔ Can add"));
        assert!(!is_testdox_suite_header("PHPUnit 10.5.0"));
        assert!(!is_testdox_suite_header("Time: 00:00.012, Memory: 8.00 MB"));
        assert!(!is_testdox_suite_header("FAILURES!"));
    }

    #[test]
    fn parse_testdox_test_line_passed() {
        let test = parse_testdox_test_line("✔ Can add numbers").unwrap();
        assert_eq!(test.name, "Can add numbers");
        assert_eq!(test.status, TestStatus::Passed);
    }

    #[test]
    fn parse_testdox_test_line_failed() {
        let test = parse_testdox_test_line("✘ Can divide by zero").unwrap();
        assert_eq!(test.name, "Can divide by zero");
        assert_eq!(test.status, TestStatus::Failed);
    }

    #[test]
    fn parse_testdox_test_line_skipped() {
        let test = parse_testdox_test_line("⚬ Pending feature").unwrap();
        assert_eq!(test.name, "Pending feature");
        assert_eq!(test.status, TestStatus::Skipped);
    }

    #[test]
    fn parse_testdox_test_line_empty() {
        assert!(parse_testdox_test_line("✔ ").is_none());
        assert!(parse_testdox_test_line("not a test").is_none());
    }

    #[test]
    fn parse_phpunit_failure_blocks() {
        let output = r#"
There was 1 failure:

1) Tests\CalculatorTest::testDivision
Failed asserting that 3 matches expected 4.

/home/user/tests/CalculatorTest.php:42

FAILURES!
Tests: 3, Assertions: 3, Failures: 1.
"#;
        let failures = parse_phpunit_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(
            failures[0].test_method,
            "Tests\\CalculatorTest::testDivision"
        );
        assert!(failures[0].message.contains("Failed asserting"));
        assert!(
            failures[0]
                .location
                .as_ref()
                .unwrap()
                .contains("CalculatorTest.php:42")
        );
    }

    #[test]
    fn parse_phpunit_multiple_failures() {
        let output = r#"
There were 2 failures:

1) Tests\MathTest::testAdd
Expected 5, got 4.

/tests/MathTest.php:10

2) Tests\MathTest::testSub
Expected 1, got 0.

/tests/MathTest.php:20

FAILURES!
"#;
        let failures = parse_phpunit_failures(output);
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].test_method, "Tests\\MathTest::testAdd");
        assert_eq!(failures[1].test_method, "Tests\\MathTest::testSub");
    }

    #[test]
    fn is_phpunit_failure_header_test() {
        assert!(is_phpunit_failure_header(
            "1) Tests\\CalculatorTest::testDivision"
        ));
        assert!(is_phpunit_failure_header("2) Tests\\AppTest::testBroken"));
        assert!(!is_phpunit_failure_header("Not a failure header"));
        assert!(!is_phpunit_failure_header(""));
    }

    #[test]
    fn is_php_file_location_test() {
        assert!(is_php_file_location("/home/user/tests/Test.php:42"));
        assert!(is_php_file_location("C:\\Users\\test\\Test.php:10"));
        assert!(!is_php_file_location("some random text"));
        assert!(!is_php_file_location("Test.php"));
    }

    #[test]
    fn enrich_with_errors_test() {
        let mut suites = vec![TestSuite {
            name: "tests".into(),
            tests: vec![
                TestCase {
                    name: "Can add".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "Can divide".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
            ],
        }];
        let failures = vec![PhpUnitFailure {
            test_method: "Tests\\MathTest::testCanDivide".into(),
            message: "Division by zero".into(),
            location: Some("/tests/MathTest.php:20".into()),
        }];
        enrich_with_errors(&mut suites, &failures);
        assert!(suites[0].tests[0].error.is_none());
        assert!(suites[0].tests[1].error.is_some());
        assert_eq!(
            suites[0].tests[1].error.as_ref().unwrap().message,
            "Division by zero"
        );
    }

    #[test]
    fn testdox_matches_test() {
        assert!(testdox_matches(
            "can add two numbers",
            "testCanAddTwoNumbers"
        ));
        assert!(testdox_matches(
            "Can add two numbers",
            "testCanAddTwoNumbers"
        ));
        assert!(!testdox_matches("can add", "testCanSubtract"));
    }

    #[test]
    fn camel_case_to_words_test() {
        assert_eq!(
            camel_case_to_words("CanAddTwoNumbers"),
            "can add two numbers"
        );
        assert_eq!(camel_case_to_words("testAdd"), "test add");
        assert_eq!(camel_case_to_words("simple"), "simple");
    }

    #[test]
    fn truncate_test() {
        assert_eq!(truncate("short", 100), "short");
        let long = "a".repeat(600);
        let truncated = truncate(&long, 500);
        assert_eq!(truncated.len(), 500);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn extract_testdox_duration_test() {
        let (name, dur) = extract_testdox_duration("Can add two numbers (0.005s)");
        assert_eq!(name, "Can add two numbers");
        assert_eq!(dur, Duration::from_millis(5));
    }

    #[test]
    fn extract_testdox_duration_ms() {
        let (name, dur) = extract_testdox_duration("Can add (50ms)");
        assert_eq!(name, "Can add");
        assert_eq!(dur, Duration::from_millis(50));
    }

    #[test]
    fn extract_testdox_duration_none() {
        let (name, dur) = extract_testdox_duration("Can add two numbers");
        assert_eq!(name, "Can add two numbers");
        assert_eq!(dur, Duration::from_millis(0));
    }

    #[test]
    fn parse_testdox_integration() {
        let stdout = r#"
PHPUnit 10.5.0 by Sebastian Bergmann and contributors.

Calculator (Tests\Calculator)
 ✔ Can add two numbers
 ✘ Can divide by zero

Time: 00:00.012, Memory: 8.00 MB

There was 1 failure:

1) Tests\Calculator::testCanDivideByZero
Failed asserting that false is true.

/tests/Calculator.php:42

FAILURES!
Tests: 2, Assertions: 2, Failures: 1.
"#;
        let adapter = PhpAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
        // The failed test should have error details
        let failed_test = result.suites[0]
            .tests
            .iter()
            .find(|t| t.status == TestStatus::Failed)
            .unwrap();
        assert!(failed_test.error.is_some());
    }
}
