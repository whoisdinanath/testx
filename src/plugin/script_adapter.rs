//! Script-based custom adapter for user-defined test frameworks.
//!
//! Users can define custom adapters in testx.toml that run arbitrary commands
//! and parse output in a standard format (JSON, JUnit XML, TAP, or line-based).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::util::duration_from_secs_safe;
use crate::adapters::{TestCase, TestError, TestRunResult, TestStatus, TestSuite};

/// Output parser type for a script adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputParser {
    /// Expects JSON matching TestRunResult schema
    Json,
    /// Expects JUnit XML output
    Junit,
    /// Expects TAP (Test Anything Protocol) output
    Tap,
    /// One test per line with status prefix
    Lines,
    /// Custom regex-based parser
    Regex(RegexParserConfig),
}

/// Configuration for regex-based output parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct RegexParserConfig {
    /// Pattern to match a passing test line
    pub pass_pattern: String,
    /// Pattern to match a failing test line
    pub fail_pattern: String,
    /// Pattern to match a skipped test line
    pub skip_pattern: Option<String>,
    /// Capture group index for the test name (1-indexed)
    pub name_group: usize,
    /// Optional capture group for duration
    pub duration_group: Option<usize>,
}

/// Definition of a custom script adapter from config.
#[derive(Debug, Clone)]
pub struct ScriptAdapterConfig {
    /// Unique adapter name
    pub name: String,
    /// File whose presence triggers detection
    pub detect_file: String,
    /// Optional detect pattern (glob) for more specific detection
    pub detect_pattern: Option<String>,
    /// Command to run
    pub command: String,
    /// Default arguments
    pub args: Vec<String>,
    /// Output parser type
    pub parser: OutputParser,
    /// Working directory relative to project root (default: ".")
    pub working_dir: Option<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
}

impl ScriptAdapterConfig {
    /// Create a minimal script adapter config.
    pub fn new(name: &str, detect_file: &str, command: &str) -> Self {
        Self {
            name: name.to_string(),
            detect_file: detect_file.to_string(),
            detect_pattern: None,
            command: command.to_string(),
            args: Vec::new(),
            parser: OutputParser::Lines,
            working_dir: None,
            env: Vec::new(),
        }
    }

    /// Set the output parser.
    pub fn with_parser(mut self, parser: OutputParser) -> Self {
        self.parser = parser;
        self
    }

    /// Set default args.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Set working directory.
    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// Check if this adapter detects at the given project directory.
    pub fn detect(&self, project_dir: &Path) -> bool {
        let detect_path = project_dir.join(&self.detect_file);
        if detect_path.exists() {
            return true;
        }

        // Check detect_pattern if set
        if let Some(ref pattern) = self.detect_pattern {
            return glob_detect(project_dir, pattern);
        }

        false
    }

    /// Get the effective working directory.
    pub fn effective_working_dir(&self, project_dir: &Path) -> PathBuf {
        match &self.working_dir {
            Some(dir) => project_dir.join(dir),
            None => project_dir.to_path_buf(),
        }
    }

    /// Build the command string with args.
    pub fn full_command(&self) -> String {
        let mut parts = vec![self.command.clone()];
        parts.extend(self.args.clone());
        parts.join(" ")
    }
}

/// Simple glob detection — checks if any file matching the pattern exists.
fn glob_detect(project_dir: &Path, pattern: &str) -> bool {
    // Simple implementation: check common patterns
    if pattern.contains('*') {
        // For now, just check if the non-glob part exists as a directory
        if let Some(base) = pattern.split('*').next() {
            let base = base.trim_end_matches('/');
            if !base.is_empty() {
                return project_dir.join(base).exists();
            }
        }
        // Fallback: try the pattern as-is
        project_dir.join(pattern).exists()
    } else {
        project_dir.join(pattern).exists()
    }
}

// ─── Output Parsers ─────────────────────────────────────────────────────

/// Parse output from a script adapter using the configured parser.
pub fn parse_script_output(
    parser: &OutputParser,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
) -> TestRunResult {
    match parser {
        OutputParser::Json => parse_json_output(stdout, stderr, exit_code),
        OutputParser::Junit => parse_junit_output(stdout, exit_code),
        OutputParser::Tap => parse_tap_output(stdout, exit_code),
        OutputParser::Lines => parse_lines_output(stdout, exit_code),
        OutputParser::Regex(config) => parse_regex_output(stdout, config, exit_code),
    }
}

/// Parse JSON-formatted test output.
fn parse_json_output(stdout: &str, _stderr: &str, exit_code: i32) -> TestRunResult {
    // Try to parse as a TestRunResult JSON
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(stdout) {
        let suites = parse_json_suites(&result);
        if !suites.is_empty() {
            return TestRunResult {
                suites,
                duration: Duration::ZERO,
                raw_exit_code: exit_code,
            };
        }
    }

    // Fallback
    fallback_result(stdout, exit_code, "json")
}

/// Extract test suites from a JSON value.
fn parse_json_suites(value: &serde_json::Value) -> Vec<TestSuite> {
    let mut suites = Vec::new();

    // Handle {"suites": [...]} format
    if let Some(arr) = value.get("suites").and_then(|v| v.as_array()) {
        for suite_val in arr {
            if let Some(suite) = parse_json_suite(suite_val) {
                suites.push(suite);
            }
        }
    }

    // Handle {"tests": [...]} format (single suite)
    if suites.is_empty()
        && let Some(arr) = value.get("tests").and_then(|v| v.as_array()) {
            let name = value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tests");
            let tests: Vec<TestCase> = arr.iter().filter_map(parse_json_test).collect();
            if !tests.is_empty() {
                suites.push(TestSuite {
                    name: name.to_string(),
                    tests,
                });
            }
        }

    // Handle [{"name": ..., "status": ...}, ...] format (flat array of tests)
    if suites.is_empty()
        && let Some(arr) = value.as_array() {
            let tests: Vec<TestCase> = arr.iter().filter_map(parse_json_test).collect();
            if !tests.is_empty() {
                suites.push(TestSuite {
                    name: "tests".to_string(),
                    tests,
                });
            }
        }

    suites
}

fn parse_json_suite(value: &serde_json::Value) -> Option<TestSuite> {
    let name = value.get("name").and_then(|v| v.as_str())?;
    let tests_arr = value.get("tests").and_then(|v| v.as_array())?;
    let tests: Vec<TestCase> = tests_arr.iter().filter_map(parse_json_test).collect();
    Some(TestSuite {
        name: name.to_string(),
        tests,
    })
}

fn parse_json_test(value: &serde_json::Value) -> Option<TestCase> {
    let name = value.get("name").and_then(|v| v.as_str())?;
    let status_str = value.get("status").and_then(|v| v.as_str())?;

    let status = match status_str.to_lowercase().as_str() {
        "passed" | "pass" | "ok" | "success" => TestStatus::Passed,
        "failed" | "fail" | "error" | "failure" => TestStatus::Failed,
        "skipped" | "skip" | "pending" | "ignored" => TestStatus::Skipped,
        _ => return None,
    };

    let duration = value
        .get("duration")
        .and_then(|v| v.as_f64())
        .map(|ms| duration_from_secs_safe(ms / 1000.0))
        .unwrap_or(Duration::ZERO);

    let error = value.get("error").and_then(|v| {
        let message = v.as_str().map(|s| s.to_string()).or_else(|| {
            v.get("message").and_then(|m| m.as_str().map(|s| s.to_string()))
        })?;
        let location = v.get("location").and_then(|l| l.as_str().map(|s| s.to_string()));
        Some(TestError { message, location })
    });

    Some(TestCase {
        name: name.to_string(),
        status,
        duration,
        error,
    })
}

/// Parse JUnit XML output.
fn parse_junit_output(stdout: &str, exit_code: i32) -> TestRunResult {
    let mut suites = Vec::new();

    // Find all <testsuite> blocks
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<testsuite") && !trimmed.starts_with("<testsuites")
            && let Some(suite) = parse_junit_suite_tag(trimmed, stdout) {
                suites.push(suite);
            }
    }

    // If no suites found, try to parse <testcase> elements directly
    if suites.is_empty() {
        let tests = parse_junit_testcases(stdout);
        if !tests.is_empty() {
            suites.push(TestSuite {
                name: "tests".to_string(),
                tests,
            });
        }
    }

    if suites.is_empty() {
        return fallback_result(stdout, exit_code, "junit");
    }

    TestRunResult {
        suites,
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

fn parse_junit_suite_tag(tag: &str, full_output: &str) -> Option<TestSuite> {
    let name = extract_xml_attr(tag, "name").unwrap_or_else(|| "tests".to_string());
    let tests = parse_junit_testcases(full_output);
    if tests.is_empty() {
        return None;
    }
    Some(TestSuite { name, tests })
}

fn parse_junit_testcases(xml: &str) -> Vec<TestCase> {
    let mut tests = Vec::new();
    let lines: Vec<&str> = xml.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("<testcase") {
            let name = extract_xml_attr(trimmed, "name").unwrap_or_else(|| "unknown".to_string());
            let time = extract_xml_attr(trimmed, "time")
                .and_then(|t| t.parse::<f64>().ok())
                .map(duration_from_secs_safe)
                .unwrap_or(Duration::ZERO);

            // Check for failure/error/skipped in subsequent lines
            let mut status = TestStatus::Passed;
            let mut error = None;

            if trimmed.ends_with("/>") {
                // Self-closing, check for nested skipped/failure check
                if trimmed.contains("<skipped") {
                    status = TestStatus::Skipped;
                }
            } else {
                // Look at following lines until </testcase>
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j].trim();
                    if inner.starts_with("</testcase") {
                        break;
                    }
                    if inner.starts_with("<failure") || inner.starts_with("<error") {
                        status = TestStatus::Failed;
                        let message = extract_xml_attr(inner, "message")
                            .unwrap_or_else(|| "Test failed".to_string());
                        error = Some(TestError {
                            message,
                            location: None,
                        });
                    }
                    if inner.starts_with("<skipped") {
                        status = TestStatus::Skipped;
                    }
                    j += 1;
                }
            }

            tests.push(TestCase {
                name,
                status,
                duration: time,
                error,
            });
        }
        i += 1;
    }

    tests
}

/// Extract an XML attribute value from an element tag.
fn extract_xml_attr(tag: &str, attr: &str) -> Option<String> {
    let search = format!("{attr}=\"");
    let start = tag.find(&search)? + search.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse TAP (Test Anything Protocol) output.
fn parse_tap_output(stdout: &str, exit_code: i32) -> TestRunResult {
    let mut tests = Vec::new();
    let mut _plan_count = 0;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Plan line: 1..N
        if let Some(rest) = trimmed.strip_prefix("1..") {
            if let Ok(n) = rest.parse::<usize>() {
                _plan_count = n;
            }
            continue;
        }

        // ok N - description
        if let Some(rest) = trimmed.strip_prefix("ok ") {
            let (name, is_skip) = parse_tap_description(rest);
            tests.push(TestCase {
                name,
                status: if is_skip {
                    TestStatus::Skipped
                } else {
                    TestStatus::Passed
                },
                duration: Duration::ZERO,
                error: None,
            });
            continue;
        }

        // not ok N - description
        if let Some(rest) = trimmed.strip_prefix("not ok ") {
            let (name, is_skip) = parse_tap_description(rest);
            let is_todo = trimmed.contains("# TODO");
            tests.push(TestCase {
                name,
                status: if is_skip || is_todo {
                    TestStatus::Skipped
                } else {
                    TestStatus::Failed
                },
                duration: Duration::ZERO,
                error: if !is_skip && !is_todo {
                    Some(TestError {
                        message: "Test failed".to_string(),
                        location: None,
                    })
                } else {
                    None
                },
            });
        }
    }

    if tests.is_empty() {
        return fallback_result(stdout, exit_code, "tap");
    }

    TestRunResult {
        suites: vec![TestSuite {
            name: "tests".to_string(),
            tests,
        }],
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

/// Parse a TAP description, extracting the test name and directive.
fn parse_tap_description(rest: &str) -> (String, bool) {
    // Strip the test number
    let after_num = rest
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| rest[i..].trim_start())
        .unwrap_or(rest);

    // Strip leading " - "
    let desc = after_num
        .strip_prefix("- ")
        .unwrap_or(after_num);

    // Check for # SKIP directive
    let is_skip = desc.contains("# SKIP") || desc.contains("# skip");

    // Remove directive from name
    let name = if let Some(idx) = desc.find(" # ") {
        desc[..idx].to_string()
    } else {
        desc.to_string()
    };

    (name, is_skip)
}

/// Parse line-based output (simplest format).
///
/// Expected format per line: `STATUS test_name` or `STATUS: test_name`
/// STATUS can be: ok, pass, passed, fail, failed, error, skip, skipped, pending
fn parse_lines_output(stdout: &str, exit_code: i32) -> TestRunResult {
    let mut tests = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(test) = parse_status_line(trimmed) {
            tests.push(test);
        }
    }

    if tests.is_empty() {
        return fallback_result(stdout, exit_code, "lines");
    }

    TestRunResult {
        suites: vec![TestSuite {
            name: "tests".to_string(),
            tests,
        }],
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

/// Parse a single status-prefixed line.
fn parse_status_line(line: &str) -> Option<TestCase> {
    let (status, rest) = parse_status_prefix(line)?;
    let name = rest.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let failed = status == TestStatus::Failed;
    Some(TestCase {
        name,
        status,
        duration: Duration::ZERO,
        error: if failed {
            Some(TestError {
                message: "Test failed".into(),
                location: None,
            })
        } else {
            None
        },
    })
}

/// Try to extract a status prefix from a line.
fn parse_status_prefix(line: &str) -> Option<(TestStatus, &str)> {
    let patterns: &[(&str, TestStatus)] = &[
        ("ok ", TestStatus::Passed),
        ("pass ", TestStatus::Passed),
        ("passed ", TestStatus::Passed),
        ("PASS ", TestStatus::Passed),
        ("PASSED ", TestStatus::Passed),
        ("OK ", TestStatus::Passed),
        ("✓ ", TestStatus::Passed),
        ("✔ ", TestStatus::Passed),
        ("fail ", TestStatus::Failed),
        ("failed ", TestStatus::Failed),
        ("error ", TestStatus::Failed),
        ("FAIL ", TestStatus::Failed),
        ("FAILED ", TestStatus::Failed),
        ("ERROR ", TestStatus::Failed),
        ("✗ ", TestStatus::Failed),
        ("✘ ", TestStatus::Failed),
        ("skip ", TestStatus::Skipped),
        ("skipped ", TestStatus::Skipped),
        ("pending ", TestStatus::Skipped),
        ("SKIP ", TestStatus::Skipped),
        ("SKIPPED ", TestStatus::Skipped),
        ("PENDING ", TestStatus::Skipped),
    ];

    for (prefix, status) in patterns {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((status.clone(), rest));
        }
    }

    // Also try "status: name" format
    let colon_patterns: &[(&str, TestStatus)] = &[
        ("ok:", TestStatus::Passed),
        ("pass:", TestStatus::Passed),
        ("fail:", TestStatus::Failed),
        ("error:", TestStatus::Failed),
        ("skip:", TestStatus::Skipped),
    ];

    for (prefix, status) in colon_patterns {
        if let Some(rest) = line.to_lowercase().strip_prefix(prefix) {
            let idx = prefix.len();
            let _ = rest; // use original line
            return Some((status.clone(), line[idx..].trim_start()));
        }
    }

    None
}

/// Parse output using custom regex patterns.
fn parse_regex_output(stdout: &str, config: &RegexParserConfig, exit_code: i32) -> TestRunResult {
    let mut tests = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(test) = try_regex_match(trimmed, &config.pass_pattern, TestStatus::Passed, config)
        {
            tests.push(test);
        } else if let Some(test) =
            try_regex_match(trimmed, &config.fail_pattern, TestStatus::Failed, config)
        {
            tests.push(test);
        } else if let Some(ref skip_pattern) = config.skip_pattern
            && let Some(test) =
                try_regex_match(trimmed, skip_pattern, TestStatus::Skipped, config)
            {
                tests.push(test);
            }
    }

    if tests.is_empty() {
        return fallback_result(stdout, exit_code, "regex");
    }

    TestRunResult {
        suites: vec![TestSuite {
            name: "tests".to_string(),
            tests,
        }],
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

/// Try to match a line against a simple pattern with capture groups.
///
/// Pattern format uses `()` for capture groups and `.*` for wildcards.
/// This is a simplified regex to avoid pulling in the regex crate.
fn try_regex_match(
    line: &str,
    pattern: &str,
    status: TestStatus,
    config: &RegexParserConfig,
) -> Option<TestCase> {
    let captures = simple_pattern_match(pattern, line)?;

    let name = captures.get(config.name_group.saturating_sub(1))?.clone();
    if name.is_empty() {
        return None;
    }

    let duration = config
        .duration_group
        .and_then(|g| captures.get(g.saturating_sub(1)))
        .and_then(|d| d.parse::<f64>().ok())
        .map(|ms| duration_from_secs_safe(ms / 1000.0))
        .unwrap_or(Duration::ZERO);

    Some(TestCase {
        name,
        status: status.clone(),
        duration,
        error: if status == TestStatus::Failed {
            Some(TestError {
                message: "Test failed".into(),
                location: None,
            })
        } else {
            None
        },
    })
}

/// Simple pattern matching with capture groups.
///
/// Supports: literal text, `(.*)` capture groups, `.*` wildcards.
/// Returns captured groups as a Vec<String>.
fn simple_pattern_match(pattern: &str, input: &str) -> Option<Vec<String>> {
    let mut captures = Vec::new();
    let mut pat_idx = 0;
    let mut inp_idx = 0;
    let pat_bytes = pattern.as_bytes();
    let inp_bytes = input.as_bytes();

    while pat_idx < pat_bytes.len() && inp_idx <= inp_bytes.len() {
        if pat_idx + 4 <= pat_bytes.len() && &pat_bytes[pat_idx..pat_idx + 4] == b"(.*)" {
            // Capture group: find the next literal after the group
            pat_idx += 4;

            // Find what comes after the capture group
            let next_literal = find_next_literal(pattern, pat_idx);

            match next_literal {
                Some(lit) => {
                    // Find the literal in the remaining input
                    let remaining = &input[inp_idx..];
                    if let Some(pos) = remaining.find(&lit) {
                        captures.push(remaining[..pos].to_string());
                        inp_idx += pos;
                    } else {
                        return None;
                    }
                }
                None => {
                    // Capture group at end of pattern, capture everything
                    captures.push(input[inp_idx..].to_string());
                    inp_idx = inp_bytes.len();
                }
            }
        } else if pat_idx + 1 < pat_bytes.len()
            && pat_bytes[pat_idx] == b'.'
            && pat_bytes[pat_idx + 1] == b'*'
        {
            // Wildcard (non-capturing): skip to next literal
            pat_idx += 2;
            let next_literal = find_next_literal(pattern, pat_idx);
            match next_literal {
                Some(lit) => {
                    let remaining = &input[inp_idx..];
                    if let Some(pos) = remaining.find(&lit) {
                        inp_idx += pos;
                    } else {
                        return None;
                    }
                }
                None => {
                    inp_idx = inp_bytes.len();
                }
            }
        } else if inp_idx < inp_bytes.len() && pat_bytes[pat_idx] == inp_bytes[inp_idx] {
            pat_idx += 1;
            inp_idx += 1;
        } else {
            return None;
        }
    }

    // Both pattern and input should be consumed
    if pat_idx == pat_bytes.len() && inp_idx == inp_bytes.len() {
        Some(captures)
    } else {
        None
    }
}

/// Find the next literal string segment in a pattern after the given index.
fn find_next_literal(pattern: &str, from: usize) -> Option<String> {
    let rest = &pattern[from..];
    if rest.is_empty() {
        return None;
    }

    let mut lit = String::new();
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b'*' {
            break;
        }
        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"(.*)" {
            break;
        }
        lit.push(bytes[i] as char);
        i += 1;
    }

    if lit.is_empty() {
        None
    } else {
        Some(lit)
    }
}

/// Generate a fallback result when parsing fails.
fn fallback_result(stdout: &str, exit_code: i32, parser_name: &str) -> TestRunResult {
    let status = if exit_code == 0 {
        TestStatus::Passed
    } else {
        TestStatus::Failed
    };

    TestRunResult {
        suites: vec![TestSuite {
            name: format!("{parser_name}-output"),
            tests: vec![TestCase {
                name: format!("test run ({parser_name} parser)"),
                status,
                duration: Duration::ZERO,
                error: if exit_code != 0 {
                    Some(TestError {
                        message: stdout
                            .lines()
                            .next()
                            .unwrap_or("Test failed")
                            .to_string(),
                        location: None,
                    })
                } else {
                    None
                },
            }],
        }],
        duration: Duration::ZERO,
        raw_exit_code: exit_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ─── ScriptAdapterConfig Tests ──────────────────────────────────────

    #[test]
    fn config_new() {
        let config = ScriptAdapterConfig::new("mytest", "Makefile", "make test");
        assert_eq!(config.name, "mytest");
        assert_eq!(config.detect_file, "Makefile");
        assert_eq!(config.command, "make test");
        assert_eq!(config.parser, OutputParser::Lines);
    }

    #[test]
    fn config_builder() {
        let config = ScriptAdapterConfig::new("mytest", "Makefile", "make test")
            .with_parser(OutputParser::Tap)
            .with_args(vec!["--verbose".into()])
            .with_working_dir("src")
            .with_env("CI", "true");

        assert_eq!(config.parser, OutputParser::Tap);
        assert_eq!(config.args, vec!["--verbose"]);
        assert_eq!(config.working_dir, Some("src".into()));
        assert_eq!(config.env, vec![("CI".into(), "true".into())]);
    }

    #[test]
    fn config_full_command() {
        let config = ScriptAdapterConfig::new("test", "f", "make test")
            .with_args(vec!["--verbose".into(), "--color".into()]);
        assert_eq!(config.full_command(), "make test --verbose --color");
    }

    #[test]
    fn config_effective_working_dir() {
        let base = PathBuf::from("/project");

        let config = ScriptAdapterConfig::new("test", "f", "cmd");
        assert_eq!(config.effective_working_dir(&base), PathBuf::from("/project"));

        let config = config.with_working_dir("src");
        assert_eq!(
            config.effective_working_dir(&base),
            PathBuf::from("/project/src")
        );
    }

    // ─── TAP Parser Tests ───────────────────────────────────────────────

    #[test]
    fn parse_tap_basic() {
        let output = "1..3\nok 1 - first test\nok 2 - second test\nnot ok 3 - third test\n";
        let result = parse_tap_output(output, 1);
        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_tap_skip() {
        let output = "1..2\nok 1 - test one\nok 2 - test two # SKIP not ready\n";
        let result = parse_tap_output(output, 0);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_tap_todo() {
        let output = "1..1\nnot ok 1 - todo test # TODO implement later\n";
        let result = parse_tap_output(output, 0);
        assert_eq!(result.total_tests(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_tap_empty() {
        let result = parse_tap_output("", 0);
        assert_eq!(result.total_tests(), 1); // fallback
    }

    #[test]
    fn parse_tap_no_plan() {
        let output = "ok 1 - works\nnot ok 2 - broken\n";
        let result = parse_tap_output(output, 1);
        assert_eq!(result.total_tests(), 2);
    }

    // ─── Lines Parser Tests ─────────────────────────────────────────────

    #[test]
    fn parse_lines_basic() {
        let output = "ok test_one\nfail test_two\nskip test_three\n";
        let result = parse_lines_output(output, 1);
        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_lines_uppercase() {
        let output = "PASS test_one\nFAIL test_two\nSKIP test_three\n";
        let result = parse_lines_output(output, 1);
        assert_eq!(result.total_tests(), 3);
    }

    #[test]
    fn parse_lines_unicode() {
        let output = "✓ test_one\n✗ test_two\n";
        let result = parse_lines_output(output, 1);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_lines_empty() {
        let result = parse_lines_output("", 0);
        assert_eq!(result.total_tests(), 1); // fallback
    }

    #[test]
    fn parse_lines_ignores_non_matching() {
        let output = "running tests...\nok test_one\nsome other output\nfail test_two\ndone";
        let result = parse_lines_output(output, 1);
        assert_eq!(result.total_tests(), 2);
    }

    // ─── JSON Parser Tests ──────────────────────────────────────────────

    #[test]
    fn parse_json_suites_format() {
        let json = r#"{
            "suites": [
                {
                    "name": "math",
                    "tests": [
                        {"name": "test_add", "status": "passed", "duration": 10},
                        {"name": "test_sub", "status": "failed", "duration": 5}
                    ]
                }
            ]
        }"#;
        let result = parse_json_output(json, "", 1);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_json_flat_tests() {
        let json = r#"{"tests": [
            {"name": "test1", "status": "pass"},
            {"name": "test2", "status": "skip"}
        ]}"#;
        let result = parse_json_output(json, "", 0);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_json_array_format() {
        let json = r#"[
            {"name": "test1", "status": "ok"},
            {"name": "test2", "status": "error"}
        ]"#;
        let result = parse_json_output(json, "", 1);
        assert_eq!(result.total_tests(), 2);
    }

    #[test]
    fn parse_json_with_errors() {
        let json = r#"{"tests": [
            {"name": "test1", "status": "failed", "error": {"message": "expected 1 got 2", "location": "test.rs:10"}}
        ]}"#;
        let result = parse_json_output(json, "", 1);
        assert_eq!(result.total_failed(), 1);
        let test = &result.suites[0].tests[0];
        assert!(test.error.is_some());
        assert_eq!(test.error.as_ref().unwrap().message, "expected 1 got 2");
    }

    #[test]
    fn parse_json_invalid() {
        let result = parse_json_output("not json {{{", "", 1);
        assert_eq!(result.total_tests(), 1); // fallback
    }

    // ─── JUnit XML Parser Tests ─────────────────────────────────────────

    #[test]
    fn parse_junit_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="math" tests="2" failures="1">
  <testcase name="test_add" classname="Math" time="0.01"/>
  <testcase name="test_div" classname="Math" time="0.02">
    <failure message="division by zero"/>
  </testcase>
</testsuite>"#;
        let result = parse_junit_output(xml, 1);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_junit_skipped() {
        let xml = r#"<testsuite name="t" tests="1">
  <testcase name="test_skip" time="0.0">
    <skipped/>
  </testcase>
</testsuite>"#;
        let result = parse_junit_output(xml, 0);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_junit_empty() {
        let result = parse_junit_output("", 0);
        assert_eq!(result.total_tests(), 1); // fallback
    }

    // ─── Regex Parser Tests ─────────────────────────────────────────────

    #[test]
    fn parse_regex_basic() {
        let config = RegexParserConfig {
            pass_pattern: "PASS: (.*)".to_string(),
            fail_pattern: "FAIL: (.*)".to_string(),
            skip_pattern: None,
            name_group: 1,
            duration_group: None,
        };
        let output = "PASS: test_one\nFAIL: test_two\nsome output\n";
        let result = parse_regex_output(output, &config, 1);
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_regex_with_skip() {
        let config = RegexParserConfig {
            pass_pattern: "[OK] (.*)".to_string(),
            fail_pattern: "[ERR] (.*)".to_string(),
            skip_pattern: Some("[SKIP] (.*)".to_string()),
            name_group: 1,
            duration_group: None,
        };
        let output = "[OK] test_one\n[SKIP] test_two\n";
        let result = parse_regex_output(output, &config, 0);
        assert_eq!(result.total_tests(), 2);
    }

    // ─── Simple Pattern Match Tests ─────────────────────────────────────

    #[test]
    fn simple_match_literal() {
        let result = simple_pattern_match("hello world", "hello world");
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn simple_match_capture() {
        let result = simple_pattern_match("PASS: (.*)", "PASS: test_one");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec!["test_one"]);
    }

    #[test]
    fn simple_match_multiple_captures() {
        let result = simple_pattern_match("(.*)=(.*)", "key=value");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec!["key", "value"]);
    }

    #[test]
    fn simple_match_wildcard() {
        let result = simple_pattern_match("hello .*!", "hello world!");
        assert!(result.is_some());
    }

    #[test]
    fn simple_match_no_match() {
        let result = simple_pattern_match("hello", "world");
        assert!(result.is_none());
    }

    #[test]
    fn simple_match_capture_with_context() {
        let result = simple_pattern_match("test (.*) in (.*)ms", "test add in 50ms");
        assert!(result.is_some());
        let caps = result.unwrap();
        assert_eq!(caps, vec!["add", "50"]);
    }

    // ─── TAP Description Parsing ────────────────────────────────────────

    #[test]
    fn tap_description_basic() {
        let (name, skip) = parse_tap_description("1 - my test");
        assert_eq!(name, "my test");
        assert!(!skip);
    }

    #[test]
    fn tap_description_skip() {
        let (name, skip) = parse_tap_description("1 - my test # SKIP not implemented");
        assert_eq!(name, "my test");
        assert!(skip);
    }

    #[test]
    fn tap_description_no_dash() {
        let (name, skip) = parse_tap_description("1 test name");
        assert_eq!(name, "test name");
        assert!(!skip);
    }

    // ─── Status Line Parsing ────────────────────────────────────────────

    #[test]
    fn status_line_pass() {
        let tc = parse_status_line("ok test_one").unwrap();
        assert_eq!(tc.name, "test_one");
        assert_eq!(tc.status, TestStatus::Passed);
    }

    #[test]
    fn status_line_fail() {
        let tc = parse_status_line("fail test_two").unwrap();
        assert_eq!(tc.name, "test_two");
        assert_eq!(tc.status, TestStatus::Failed);
    }

    #[test]
    fn status_line_skip() {
        let tc = parse_status_line("skip test_three").unwrap();
        assert_eq!(tc.name, "test_three");
        assert_eq!(tc.status, TestStatus::Skipped);
    }

    #[test]
    fn status_line_no_match() {
        assert!(parse_status_line("some random text").is_none());
    }

    #[test]
    fn status_line_empty_name() {
        assert!(parse_status_line("ok ").is_none());
    }

    // ─── XML Attr Extraction ────────────────────────────────────────────

    #[test]
    fn xml_attr_basic() {
        assert_eq!(
            extract_xml_attr(r#"<test name="hello" time="1.5">"#, "name"),
            Some("hello".into())
        );
    }

    #[test]
    fn xml_attr_missing() {
        assert_eq!(extract_xml_attr("<test>", "name"), None);
    }

    // ─── Fallback Result Tests ──────────────────────────────────────────

    #[test]
    fn fallback_pass() {
        let result = fallback_result("all good", 0, "test");
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.raw_exit_code, 0);
    }

    #[test]
    fn fallback_fail() {
        let result = fallback_result("something failed", 1, "test");
        assert_eq!(result.total_failed(), 1);
        assert!(result.suites[0].tests[0].error.is_some());
    }

    // ─── Integration: parse_script_output ───────────────────────────────

    #[test]
    fn script_output_delegates_to_tap() {
        let output = "1..2\nok 1 - a\nnot ok 2 - b\n";
        let result = parse_script_output(&OutputParser::Tap, output, "", 1);
        assert_eq!(result.total_tests(), 2);
    }

    #[test]
    fn script_output_delegates_to_lines() {
        let output = "PASS test1\nFAIL test2\n";
        let result = parse_script_output(&OutputParser::Lines, output, "", 1);
        assert_eq!(result.total_tests(), 2);
    }

    #[test]
    fn script_output_delegates_to_json() {
        let output = r#"[{"name": "t1", "status": "passed"}]"#;
        let result = parse_script_output(&OutputParser::Json, output, "", 0);
        assert_eq!(result.total_tests(), 1);
        assert_eq!(result.total_passed(), 1);
    }
}
