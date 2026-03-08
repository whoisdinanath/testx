use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

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

        Some(DetectionResult {
            language: "PHP".into(),
            framework: "PHPUnit".into(),
            confidence: 0.9,
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

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);

        let suites = parse_phpunit_output(&combined, exit_code);
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
                return Some(Duration::from_secs_f64(mins * 60.0 + secs));
            }
        }
    }
    None
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
    fn parse_phpunit_failures() {
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
}
