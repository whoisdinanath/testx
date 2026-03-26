//! GitHub Actions reporter plugin.
//!
//! Outputs GitHub Actions workflow commands for annotations,
//! grouping, and step summaries.

use std::fmt::Write;
use std::time::Duration;

use crate::adapters::{TestRunResult, TestStatus, TestSuite};
use crate::error;
use crate::events::TestEvent;
use crate::plugin::Plugin;

/// GitHub Actions reporter configuration.
#[derive(Debug, Clone)]
pub struct GithubConfig {
    /// Emit `::error` / `::warning` annotations for failures
    pub annotations: bool,
    /// Use `::group` / `::endgroup` for suite output
    pub groups: bool,
    /// Write a step summary to `$GITHUB_STEP_SUMMARY`
    pub step_summary: bool,
    /// Inject problem matcher pattern
    pub problem_matcher: bool,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            annotations: true,
            groups: true,
            step_summary: true,
            problem_matcher: false,
        }
    }
}

/// GitHub Actions reporter plugin.
pub struct GithubReporter {
    config: GithubConfig,
    collected: Vec<String>,
}

impl GithubReporter {
    pub fn new(config: GithubConfig) -> Self {
        Self {
            config,
            collected: Vec::new(),
        }
    }

    /// Get the collected output lines.
    pub fn output(&self) -> &[String] {
        &self.collected
    }
}

impl Plugin for GithubReporter {
    fn name(&self) -> &str {
        "github"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_event(&mut self, _event: &TestEvent) -> error::Result<()> {
        Ok(())
    }

    fn on_result(&mut self, result: &TestRunResult) -> error::Result<()> {
        self.collected = generate_github_output(result, &self.config);
        Ok(())
    }
}

/// Generate GitHub Actions workflow commands from test results.
pub fn generate_github_output(result: &TestRunResult, config: &GithubConfig) -> Vec<String> {
    let mut lines = Vec::new();

    if config.problem_matcher {
        write_problem_matcher(&mut lines);
    }

    for suite in &result.suites {
        if config.groups {
            write_group(&mut lines, suite);
        }
        if config.annotations {
            write_annotations(&mut lines, suite);
        }
    }

    if config.step_summary {
        write_step_summary_commands(&mut lines, result);
    }

    // Final outcome line
    let status = if result.is_success() {
        "passed"
    } else {
        "failed"
    };
    lines.push(format!(
        "::notice::testx: {} tests {status} ({} passed, {} failed, {} skipped) in {}",
        result.total_tests(),
        result.total_passed(),
        result.total_failed(),
        result.total_skipped(),
        format_duration(result.duration),
    ));

    lines
}

fn write_problem_matcher(lines: &mut Vec<String>) {
    lines.push("::add-matcher::testx-matcher.json".into());
}

fn write_group(lines: &mut Vec<String>, suite: &TestSuite) {
    let icon = if suite.is_passed() { "✅" } else { "❌" };
    lines.push(format!(
        "::group::{icon} {} ({} tests, {} failed)",
        suite.name,
        suite.tests.len(),
        suite.failed(),
    ));

    for test in &suite.tests {
        let icon = match test.status {
            TestStatus::Passed => "✅",
            TestStatus::Failed => "❌",
            TestStatus::Skipped => "⏭️",
        };
        lines.push(format!("  {icon} {} ({:?})", test.name, test.duration));
    }

    lines.push("::endgroup::".into());
}

fn write_annotations(lines: &mut Vec<String>, suite: &TestSuite) {
    for test in suite.failures() {
        let msg = test
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "test failed".into());

        // Extract file/line from error location if available
        if let Some(ref error) = test.error
            && let Some(ref loc) = error.location
                && let Some((file, line)) = parse_location(loc) {
                    lines.push(format!(
                        "::error file={file},line={line},title={}::{}",
                        escape_workflow_value(&test.name),
                        escape_workflow_value(&msg),
                    ));
                    continue;
                }

        lines.push(format!(
            "::error title={} ({})::{msg}",
            escape_workflow_value(&test.name),
            suite.name,
        ));
    }
}

fn write_step_summary_commands(lines: &mut Vec<String>, result: &TestRunResult) {
    let mut md = String::with_capacity(1024);
    let icon = if result.is_success() {
        "✅ Passed"
    } else {
        "❌ Failed"
    };

    let _ = writeln!(md, "### Test Results — {icon}");
    md.push('\n');
    let _ = writeln!(
        md,
        "| Total | Passed | Failed | Skipped | Duration |"
    );
    let _ = writeln!(md, "| ----- | ------ | ------ | ------- | -------- |");
    let _ = writeln!(
        md,
        "| {} | {} | {} | {} | {} |",
        result.total_tests(),
        result.total_passed(),
        result.total_failed(),
        result.total_skipped(),
        format_duration(result.duration),
    );

    if result.total_failed() > 0 {
        md.push('\n');
        let _ = writeln!(md, "#### Failures");
        md.push('\n');
        for suite in &result.suites {
            for test in suite.failures() {
                let msg = test
                    .error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "test failed".into());
                let _ = writeln!(md, "- **{}::{}**: {}", suite.name, test.name, msg);
            }
        }
    }

    // Output as GITHUB_STEP_SUMMARY echo commands
    for line in md.lines() {
        let escaped = line.replace('`', "\\`");
        lines.push(format!("echo '{escaped}' >> $GITHUB_STEP_SUMMARY"));
    }
}

/// Parse a location string like "file.rs:42" or "file.rs:42:10".
fn parse_location(loc: &str) -> Option<(String, String)> {
    // Try "file:line:col" first, then "file:line"
    let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
    if parts.len() == 3
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].chars().all(|c| c.is_ascii_digit())
    {
        // file:line:col — return (file, line)
        return Some((parts[2].to_string(), parts[1].to_string()));
    }
    if parts.len() >= 2 && parts[0].chars().all(|c| c.is_ascii_digit()) && !parts[0].is_empty() {
        // file:line
        let line = parts[0];
        let file = &loc[..loc.len() - line.len() - 1];
        return Some((file.to_string(), line.to_string()));
    }
    None
}

/// Escape a string for use in workflow commands (`%0A`, `%25`, etc.).
fn escape_workflow_value(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        "<1ms".to_string()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestError, TestSuite};

    fn make_test(name: &str, status: TestStatus, ms: u64) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(ms),
            error: None,
        }
    }

    fn make_failed_test(name: &str, ms: u64, msg: &str, loc: Option<&str>) -> TestCase {
        TestCase {
            name: name.into(),
            status: TestStatus::Failed,
            duration: Duration::from_millis(ms),
            error: Some(TestError {
                message: msg.into(),
                location: loc.map(String::from),
            }),
        }
    }

    fn make_result() -> TestRunResult {
        TestRunResult {
            suites: vec![
                TestSuite {
                    name: "math".into(),
                    tests: vec![
                        make_test("add", TestStatus::Passed, 10),
                        make_failed_test("div", 5, "divide by zero", Some("math.rs:42")),
                    ],
                },
                TestSuite {
                    name: "strings".into(),
                    tests: vec![
                        make_test("concat", TestStatus::Passed, 15),
                        make_test("upper", TestStatus::Skipped, 0),
                    ],
                },
            ],
            duration: Duration::from_millis(300),
            raw_exit_code: 1,
        }
    }

    #[test]
    fn github_groups() {
        let lines = generate_github_output(&make_result(), &GithubConfig::default());
        assert!(lines.iter().any(|l| l.starts_with("::group::")));
        assert!(lines.iter().any(|l| l == "::endgroup::"));
    }

    #[test]
    fn github_annotations() {
        let lines = generate_github_output(&make_result(), &GithubConfig::default());
        let error_lines: Vec<_> = lines.iter().filter(|l| l.starts_with("::error")).collect();
        assert_eq!(error_lines.len(), 1);
        assert!(error_lines[0].contains("file=math.rs"));
        assert!(error_lines[0].contains("line=42"));
    }

    #[test]
    fn github_annotation_without_location() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "t".into(),
                tests: vec![make_failed_test("f1", 1, "boom", None)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 1,
        };
        let lines = generate_github_output(&result, &GithubConfig::default());
        let error_lines: Vec<_> = lines.iter().filter(|l| l.starts_with("::error")).collect();
        assert_eq!(error_lines.len(), 1);
        assert!(error_lines[0].contains("title=f1"));
    }

    #[test]
    fn github_step_summary() {
        let lines = generate_github_output(&make_result(), &GithubConfig::default());
        let summary_lines: Vec<_> = lines
            .iter()
            .filter(|l| l.contains("GITHUB_STEP_SUMMARY"))
            .collect();
        assert!(!summary_lines.is_empty());
        assert!(summary_lines.iter().any(|l| l.contains("Test Results")));
    }

    #[test]
    fn github_notice_line() {
        let lines = generate_github_output(&make_result(), &GithubConfig::default());
        let notice = lines.iter().find(|l| l.starts_with("::notice::")).unwrap();
        assert!(notice.contains("4 tests failed"));
    }

    #[test]
    fn github_passing_notice() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "t".into(),
                tests: vec![make_test("t1", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let lines = generate_github_output(&result, &GithubConfig::default());
        let notice = lines.iter().find(|l| l.starts_with("::notice::")).unwrap();
        assert!(notice.contains("passed"));
    }

    #[test]
    fn github_problem_matcher() {
        let config = GithubConfig {
            problem_matcher: true,
            ..Default::default()
        };
        let lines = generate_github_output(&make_result(), &config);
        assert!(lines[0].contains("add-matcher"));
    }

    #[test]
    fn github_no_groups() {
        let config = GithubConfig {
            groups: false,
            ..Default::default()
        };
        let lines = generate_github_output(&make_result(), &config);
        assert!(!lines.iter().any(|l| l.starts_with("::group::")));
    }

    #[test]
    fn github_plugin_trait() {
        let mut reporter = GithubReporter::new(GithubConfig::default());
        assert_eq!(reporter.name(), "github");
        reporter.on_result(&make_result()).unwrap();
        assert!(!reporter.output().is_empty());
    }

    #[test]
    fn parse_location_simple() {
        let (file, line) = parse_location("test.rs:42").unwrap();
        assert_eq!(file, "test.rs");
        assert_eq!(line, "42");
    }

    #[test]
    fn parse_location_with_column() {
        let (file, line) = parse_location("test.rs:42:10").unwrap();
        assert_eq!(file, "test.rs");
        assert_eq!(line, "42");
    }

    #[test]
    fn parse_location_invalid() {
        assert!(parse_location("no_colon").is_none());
    }

    #[test]
    fn escape_workflow_newlines() {
        let escaped = escape_workflow_value("line1\nline2");
        assert_eq!(escaped, "line1%0Aline2");
    }

    #[test]
    fn escape_workflow_percent() {
        let escaped = escape_workflow_value("100%");
        assert_eq!(escaped, "100%25");
    }
}
