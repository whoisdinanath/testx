use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus,
    TestSuite,
};

pub struct RubyAdapter;

impl Default for RubyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RubyAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Detect test framework: rspec or minitest
    fn detect_framework(project_dir: &Path) -> Option<&'static str> {
        // RSpec
        if project_dir.join(".rspec").exists() {
            return Some("rspec");
        }
        if project_dir.join("spec").is_dir() {
            return Some("rspec");
        }

        // Check Gemfile for test framework
        let gemfile = project_dir.join("Gemfile");
        if gemfile.exists() {
            if let Ok(content) = std::fs::read_to_string(&gemfile) {
                if content.contains("rspec") {
                    return Some("rspec");
                }
                if content.contains("minitest") {
                    return Some("minitest");
                }
            }
            // Has Gemfile but no specific test framework detected
            return Some("minitest"); // Ruby's default
        }

        // Rakefile with test task
        let rakefile = project_dir.join("Rakefile");
        if rakefile.exists() {
            return Some("minitest");
        }

        // test/ directory exists with .rb files inside (not just any test/ dir)
        if project_dir.join("test").is_dir()
            && let Ok(entries) = std::fs::read_dir(project_dir.join("test"))
        {
            let has_ruby_files = entries
                .filter_map(|e| e.ok())
                .any(|e| e.path().extension().is_some_and(|ext| ext == "rb"));
            if has_ruby_files {
                return Some("minitest");
            }
        }

        None
    }

    fn has_bundler(project_dir: &Path) -> bool {
        project_dir.join("Gemfile").exists()
    }
}

impl TestAdapter for RubyAdapter {
    fn name(&self) -> &str {
        "Ruby"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("ruby").is_err() {
            return Some("ruby not found. Install Ruby.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let framework = Self::detect_framework(project_dir)?;

        let has_spec_or_test =
            project_dir.join("spec").is_dir() || project_dir.join("test").is_dir();
        let has_lock = project_dir.join("Gemfile.lock").exists();
        let has_runner = which::which("ruby").is_ok();

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, has_spec_or_test)
            .signal(0.15, has_lock)
            .signal(0.10, has_runner)
            .finish();

        Some(DetectionResult {
            language: "Ruby".into(),
            framework: framework.into(),
            confidence,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let framework = Self::detect_framework(project_dir).unwrap_or("rspec");
        let use_bundler = Self::has_bundler(project_dir);

        let mut cmd;

        match framework {
            "rspec" => {
                if use_bundler {
                    cmd = Command::new("bundle");
                    cmd.arg("exec");
                    cmd.arg("rspec");
                } else {
                    cmd = Command::new("rspec");
                }
            }
            _ => {
                // minitest
                if use_bundler {
                    cmd = Command::new("bundle");
                    cmd.arg("exec");
                    cmd.arg("rake");
                    cmd.arg("test");
                } else {
                    cmd = Command::new("rake");
                    cmd.arg("test");
                }
            }
        }

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);

        // Try verbose parsing first (--format documentation / --verbose)
        let suites = if combined.contains("example") || combined.contains("Example") {
            let verbose = parse_rspec_verbose(&combined);
            if verbose.iter().any(|s| !s.tests.is_empty()) {
                verbose
            } else {
                parse_rspec_output(&combined, exit_code)
            }
        } else {
            let verbose = parse_minitest_verbose(&combined);
            if verbose.iter().any(|s| !s.tests.is_empty()) {
                verbose
            } else {
                parse_minitest_output(&combined, exit_code)
            }
        };

        // Enrich failed tests with error details from failure blocks
        let failures = parse_rspec_failures(&combined);
        let minitest_failures = parse_minitest_failures(&combined);

        let suites = enrich_with_errors(suites, &failures, &minitest_failures);

        let duration = parse_ruby_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse RSpec output.
///
/// Format:
/// ```text
/// ..F.*
///
/// Failures:
///
///   1) Calculator adds two numbers
///      Failure/Error: expect(sum).to eq(5)
///        expected: 5
///             got: 4
///
/// Finished in 0.012 seconds (files took 0.1 seconds to load)
/// 5 examples, 1 failure, 1 pending
/// ```
fn parse_rspec_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    // Parse the summary line: "5 examples, 1 failure, 1 pending"
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("example")
            && (trimmed.contains("failure") || trimmed.contains("pending"))
        {
            let parts: Vec<&str> = trimmed.split(',').collect();
            let mut examples = 0usize;
            let mut failures = 0usize;
            let mut pending = 0usize;

            for part in &parts {
                let part = part.trim();
                let words: Vec<&str> = part.split_whitespace().collect();
                if words.len() >= 2 {
                    let count: usize = words[0].parse().unwrap_or(0);
                    if words[1].starts_with("example") {
                        examples = count;
                    } else if words[1].starts_with("failure") {
                        failures = count;
                    } else if words[1].starts_with("pending") {
                        pending = count;
                    }
                }
            }

            let passed = examples.saturating_sub(failures + pending);
            for i in 0..passed {
                tests.push(TestCase {
                    name: format!("example_{}", i + 1),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                });
            }
            for i in 0..failures {
                tests.push(TestCase {
                    name: format!("failed_example_{}", i + 1),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                });
            }
            for i in 0..pending {
                tests.push(TestCase {
                    name: format!("pending_example_{}", i + 1),
                    status: TestStatus::Skipped,
                    duration: Duration::from_millis(0),
                    error: None,
                });
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
        name: "spec".into(),
        tests,
    }]
}

/// Parse Minitest output.
///
/// Format:
/// ```text
/// Run options: --seed 12345
///
/// # Running:
///
/// ..F.
///
/// Finished in 0.001234s, 3000.0 runs/s, 3000.0 assertions/s.
///
/// 4 runs, 4 assertions, 1 failures, 0 errors, 0 skips
/// ```
fn parse_minitest_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        // "4 runs, 4 assertions, 1 failures, 0 errors, 0 skips"
        if trimmed.contains("runs,") && trimmed.contains("assertions,") {
            let mut runs = 0usize;
            let mut failures = 0usize;
            let mut errors = 0usize;
            let mut skips = 0usize;

            for part in trimmed.split(',') {
                let part = part.trim();
                let words: Vec<&str> = part.split_whitespace().collect();
                if words.len() >= 2 {
                    let count: usize = words[0].parse().unwrap_or(0);
                    if words[1].starts_with("run") {
                        runs = count;
                    } else if words[1].starts_with("failure") {
                        failures = count;
                    } else if words[1].starts_with("error") {
                        errors = count;
                    } else if words[1].starts_with("skip") {
                        skips = count;
                    }
                }
            }

            let failed = failures + errors;
            let passed = runs.saturating_sub(failed + skips);

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
            for i in 0..skips {
                tests.push(TestCase {
                    name: format!("skipped_test_{}", i + 1),
                    status: TestStatus::Skipped,
                    duration: Duration::from_millis(0),
                    error: None,
                });
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

fn parse_ruby_duration(output: &str) -> Option<Duration> {
    for line in output.lines() {
        // RSpec: "Finished in 0.012 seconds"
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
        // Minitest: "Finished in 0.001234s,"
        if line.contains("Finished in")
            && line.contains("runs/s")
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

// ─── Verbose RSpec Parser (--format documentation) ──────────────────────────

/// Parse RSpec verbose/documentation format output.
///
/// ```text
/// User authentication
///   with valid credentials
///     allows login (0.02s)
///   with invalid credentials
///     shows error message (0.01s)
///     increments attempt counter (FAILED - 1)
/// ```
fn parse_rspec_verbose(output: &str) -> Vec<TestSuite> {
    let mut suites: Vec<TestSuite> = Vec::new();
    let mut current_context: Vec<String> = Vec::new();
    let mut current_tests: Vec<TestCase> = Vec::new();
    let mut current_suite_name = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip empty lines and non-test lines
        if trimmed.is_empty()
            || trimmed.starts_with("Finished in")
            || trimmed.starts_with("Failures:")
            || trimmed.starts_with("Pending:")
            || trimmed.contains("example")
                && (trimmed.contains("failure") || trimmed.contains("pending"))
        {
            continue;
        }

        // Detect indentation level
        let indent = line.len() - line.trim_start().len();

        // Test result line: ends with time or FAILED or PENDING
        if is_rspec_test_line(trimmed) {
            let (name, status, duration) = parse_rspec_test_line(trimmed);

            let full_name = if current_context.is_empty() {
                name.clone()
            } else {
                format!("{} {}", current_context.join(" "), name)
            };

            current_tests.push(TestCase {
                name: full_name,
                status,
                duration,
                error: None,
            });
        } else if !trimmed.starts_with('#')
            && !trimmed.starts_with("1)")
            && !trimmed.starts_with("2)")
            && !trimmed.starts_with("3)")
            && !trimmed.contains("Failure/Error")
            && !trimmed.contains("expected:")
            && !trimmed.contains("got:")
            && !trimmed.starts_with("./")
        {
            // Context/describe line: adjust context stack based on indent
            let level = indent / 2;
            while current_context.len() > level {
                current_context.pop();
            }

            // If we're at top level and have tests, store the previous suite
            if level == 0 && !current_tests.is_empty() {
                suites.push(TestSuite {
                    name: if current_suite_name.is_empty() {
                        "spec".to_string()
                    } else {
                        current_suite_name.clone()
                    },
                    tests: std::mem::take(&mut current_tests),
                });
            }

            if level == 0 {
                current_suite_name = trimmed.to_string();
            }

            if current_context.len() == level {
                current_context.push(trimmed.to_string());
            }
        }
    }

    // Store remaining tests
    if !current_tests.is_empty() {
        suites.push(TestSuite {
            name: if current_suite_name.is_empty() {
                "spec".to_string()
            } else {
                current_suite_name
            },
            tests: current_tests,
        });
    }

    suites
}

/// Check if a line looks like an RSpec test result.
fn is_rspec_test_line(line: &str) -> bool {
    // Matches patterns like:
    //   "allows login (0.02s)"
    //   "shows error message (FAILED - 1)"
    //   "is pending (PENDING: Not yet implemented)"
    line.contains("(FAILED")
        || line.contains("(PENDING")
        || (line.ends_with(')') && line.contains('(') && line.contains("s)"))
}

/// Parse a single RSpec test result line.
fn parse_rspec_test_line(line: &str) -> (String, TestStatus, Duration) {
    if line.contains("(FAILED") {
        let name = line
            .split("(FAILED")
            .next()
            .unwrap_or(line)
            .trim()
            .to_string();
        return (name, TestStatus::Failed, Duration::from_millis(0));
    }

    if line.contains("(PENDING") {
        let name = line
            .split("(PENDING")
            .next()
            .unwrap_or(line)
            .trim()
            .to_string();
        return (name, TestStatus::Skipped, Duration::from_millis(0));
    }

    // Try to extract duration: "test name (0.02s)" or "test name (0.02 seconds)"
    if let Some(paren_idx) = line.rfind('(') {
        let name = line[..paren_idx].trim().to_string();
        let time_part = &line[paren_idx + 1..];
        let duration = parse_rspec_inline_duration(time_part);
        return (name, TestStatus::Passed, duration);
    }

    (
        line.trim().to_string(),
        TestStatus::Passed,
        Duration::from_millis(0),
    )
}

/// Parse inline duration from "0.02s)" or "0.02 seconds)".
fn parse_rspec_inline_duration(s: &str) -> Duration {
    let num_str: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if let Ok(secs) = num_str.parse::<f64>() {
        duration_from_secs_safe(secs)
    } else {
        Duration::from_millis(0)
    }
}

// ─── Verbose Minitest Parser (--verbose) ────────────────────────────────────

/// Parse Minitest verbose output.
///
/// ```text
/// TestUser#test_name_returns_full_name = 0.01 s = .
/// TestUser#test_email_validation = 0.00 s = F
/// TestUser#test_age_is_positive = 0.00 s = S
/// ```
fn parse_minitest_verbose(output: &str) -> Vec<TestSuite> {
    let mut suites_map: std::collections::HashMap<String, Vec<TestCase>> =
        std::collections::HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Format: "ClassName#test_name = TIME s = STATUS"
        if let Some((class_test, rest)) = trimmed.split_once(" = ")
            && let Some((class, test)) = class_test.split_once('#')
        {
            // Extract duration and status
            let (duration, status) = parse_minitest_verbose_result(rest);

            suites_map
                .entry(class.to_string())
                .or_default()
                .push(TestCase {
                    name: test.to_string(),
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

/// Parse "0.01 s = ." or "0.00 s = F" from minitest verbose.
fn parse_minitest_verbose_result(s: &str) -> (Duration, TestStatus) {
    let parts: Vec<&str> = s.split('=').collect();

    let duration = if let Some(time_part) = parts.first() {
        let num_str: String = time_part
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        num_str
            .parse::<f64>()
            .map(duration_from_secs_safe)
            .unwrap_or(Duration::from_millis(0))
    } else {
        Duration::from_millis(0)
    };

    let status = if let Some(status_part) = parts.get(1) {
        let status_char = status_part.trim();
        match status_char {
            "." => TestStatus::Passed,
            "F" => TestStatus::Failed,
            "E" => TestStatus::Failed,
            "S" => TestStatus::Skipped,
            _ => TestStatus::Passed,
        }
    } else {
        TestStatus::Passed
    };

    (duration, status)
}

// ─── RSpec Failure Block Parser ─────────────────────────────────────────────

/// A parsed failure from RSpec output.
#[derive(Debug, Clone)]
struct RspecFailure {
    /// Full test name, e.g. "User authentication with invalid credentials increments attempt counter"
    name: String,
    /// Error message, e.g. "expected: 1\n     got: 0"
    message: String,
    /// Location, e.g. "./spec/auth_spec.rb:25:in `block (3 levels)'"
    location: Option<String>,
}

/// Parse RSpec failure blocks.
///
/// ```text
/// Failures:
///
///   1) User authentication with invalid credentials increments attempt counter
///      Failure/Error: expect(counter).to eq(1)
///
///        expected: 1
///             got: 0
///
///      # ./spec/auth_spec.rb:25:in `block (3 levels)'
/// ```
fn parse_rspec_failures(output: &str) -> Vec<RspecFailure> {
    let mut failures = Vec::new();
    let mut in_failures_section = false;
    let mut current_name: Option<String> = None;
    let mut current_message = Vec::new();
    let mut current_location: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed == "Failures:" {
            in_failures_section = true;
            continue;
        }

        if !in_failures_section {
            continue;
        }

        // End of failures section
        if trimmed.starts_with("Finished in") || trimmed.starts_with("Pending:") {
            // Save last failure
            if let Some(name) = current_name.take() {
                failures.push(RspecFailure {
                    name,
                    message: current_message.join("\n").trim().to_string(),
                    location: current_location.take(),
                });
            }
            break;
        }

        // Numbered failure: "1) Description here"
        if let Some(rest) = strip_failure_number(trimmed) {
            // Save previous failure
            if let Some(name) = current_name.take() {
                failures.push(RspecFailure {
                    name,
                    message: current_message.join("\n").trim().to_string(),
                    location: current_location.take(),
                });
            }
            current_name = Some(rest.to_string());
            current_message.clear();
            current_location = None;
            continue;
        }

        if current_name.is_some() {
            // Location line: "# ./spec/file.rb:25"
            if trimmed.starts_with("# ./") || trimmed.starts_with("# /") {
                current_location = Some(trimmed.trim_start_matches("# ").to_string());
            } else if trimmed.starts_with("Failure/Error:") {
                let msg = trimmed.strip_prefix("Failure/Error:").unwrap_or("").trim();
                if !msg.is_empty() {
                    current_message.push(msg.to_string());
                }
            } else if !trimmed.is_empty() {
                current_message.push(trimmed.to_string());
            }
        }
    }

    // Save last failure if section ended without "Finished in"
    if let Some(name) = current_name {
        failures.push(RspecFailure {
            name,
            message: current_message.join("\n").trim().to_string(),
            location: current_location,
        });
    }

    failures
}

/// Strip failure number prefix like "1) ", "12) ", etc.
fn strip_failure_number(s: &str) -> Option<&str> {
    let mut chars = s.chars();
    let first = chars.next()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let rest: String = chars.collect();
    if let Some(idx) = rest.find(") ") {
        let before = &rest[..idx];
        if before.chars().all(|c| c.is_ascii_digit()) {
            return Some(s[idx + 2 + 1..].trim_start());
        }
    }
    None
}

// ─── Minitest Failure Block Parser ──────────────────────────────────────────

/// A parsed failure from Minitest output.
#[derive(Debug, Clone)]
struct MinitestFailure {
    /// Test name, e.g. "test_email_validation"
    name: String,
    /// Error message
    message: String,
    /// Location
    location: Option<String>,
}

/// Parse Minitest failure blocks.
///
/// ```text
///   1) Failure:
/// TestUser#test_email_validation [test/user_test.rb:15]:
/// Expected: true
///   Actual: false
///
///   2) Error:
/// TestCalc#test_divide [test/calc_test.rb:8]:
/// ZeroDivisionError: divided by 0
///     test/calc_test.rb:9:in `test_divide'
/// ```
fn parse_minitest_failures(output: &str) -> Vec<MinitestFailure> {
    let mut failures = Vec::new();
    let mut in_failure = false;
    let mut current_name: Option<String> = None;
    let mut current_message = Vec::new();
    let mut current_location: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        // Detect failure/error header
        if (trimmed.ends_with("Failure:") || trimmed.ends_with("Error:"))
            && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            // Save previous
            if let Some(name) = current_name.take() {
                failures.push(MinitestFailure {
                    name,
                    message: current_message.join("\n").trim().to_string(),
                    location: current_location.take(),
                });
            }
            in_failure = true;
            current_message.clear();
            current_location = None;
            continue;
        }

        if in_failure && current_name.is_none() {
            // Next line after "Failure:" should be "ClassName#test_name [location]:"
            if trimmed.contains('#') && trimmed.contains('[') {
                if let Some(bracket_idx) = trimmed.find('[') {
                    let name_part = trimmed[..bracket_idx].trim();
                    // Extract just the test method name
                    let test_name = if let Some(hash_idx) = name_part.find('#') {
                        &name_part[hash_idx + 1..]
                    } else {
                        name_part
                    };
                    current_name = Some(test_name.to_string());

                    // Extract location from [path:line]
                    if let Some(close_bracket) = trimmed.find(']') {
                        let loc = &trimmed[bracket_idx + 1..close_bracket];
                        current_location = Some(loc.to_string());
                    }
                }
            } else if !trimmed.is_empty() {
                current_name = Some(trimmed.to_string());
            }
            continue;
        }

        if in_failure && current_name.is_some() {
            if trimmed.is_empty() {
                // End of this failure block
                if let Some(name) = current_name.take() {
                    failures.push(MinitestFailure {
                        name,
                        message: current_message.join("\n").trim().to_string(),
                        location: current_location.take(),
                    });
                }
                in_failure = false;
                current_message.clear();
            } else {
                current_message.push(trimmed.to_string());
            }
        }
    }

    // Save last
    if let Some(name) = current_name {
        failures.push(MinitestFailure {
            name,
            message: current_message.join("\n").trim().to_string(),
            location: current_location,
        });
    }

    failures
}

// ─── Error Enrichment ───────────────────────────────────────────────────────

/// Enrich test cases with error details from parsed failure blocks.
fn enrich_with_errors(
    suites: Vec<TestSuite>,
    rspec_failures: &[RspecFailure],
    minitest_failures: &[MinitestFailure],
) -> Vec<TestSuite> {
    suites
        .into_iter()
        .map(|suite| {
            let tests = suite
                .tests
                .into_iter()
                .map(|mut test| {
                    if test.status == TestStatus::Failed && test.error.is_none() {
                        // Try to find matching RSpec failure
                        if let Some(failure) = rspec_failures
                            .iter()
                            .find(|f| f.name.contains(&test.name) || test.name.contains(&f.name))
                        {
                            test.error = Some(TestError {
                                message: truncate_message(&failure.message, 500),
                                location: failure.location.clone(),
                            });
                        }
                        // Try to find matching Minitest failure
                        else if let Some(failure) = minitest_failures
                            .iter()
                            .find(|f| f.name == test.name || test.name.contains(&f.name))
                        {
                            test.error = Some(TestError {
                                message: truncate_message(&failure.message, 500),
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

/// Truncate a message to max_len characters.
fn truncate_message(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rspec_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".rspec"), "--format documentation\n").unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Ruby");
        assert_eq!(det.framework, "rspec");
    }

    #[test]
    fn detect_rspec_via_gemfile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\ngem 'rspec'\n",
        )
        .unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "rspec");
    }

    #[test]
    fn detect_minitest_via_gemfile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\ngem 'minitest'\n",
        )
        .unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "minitest");
    }

    #[test]
    fn detect_no_ruby() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = RubyAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_rspec_output_test() {
        let stdout = r#"
..F.*

Failures:

  1) Calculator adds two numbers
     Failure/Error: expect(sum).to eq(5)
       expected: 5
            got: 4

Finished in 0.012 seconds (files took 0.1 seconds to load)
5 examples, 1 failure, 1 pending
"#;
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 3);
        assert_eq!(result.total_failed(), 1);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_rspec_all_pass() {
        let stdout = "Finished in 0.005 seconds\n3 examples, 0 failures\n";
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 3);
        assert!(result.is_success());
    }

    #[test]
    fn parse_minitest_output_test() {
        let stdout = r#"
Run options: --seed 12345

# Running:

..F.

Finished in 0.001234s, 3000.0 runs/s, 3000.0 assertions/s.

4 runs, 4 assertions, 1 failures, 0 errors, 0 skips
"#;
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 4);
        assert_eq!(result.total_passed(), 3);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_minitest_all_pass() {
        let stdout = "4 runs, 4 assertions, 0 failures, 0 errors, 0 skips\n";
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 4);
        assert_eq!(result.total_passed(), 4);
        assert!(result.is_success());
    }

    #[test]
    fn parse_ruby_empty_output() {
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_rspec_duration_test() {
        assert_eq!(
            parse_ruby_duration("Finished in 0.012 seconds (files took 0.1 seconds to load)"),
            Some(Duration::from_millis(12))
        );
    }

    // ─── Verbose RSpec Tests ────────────────────────────────────────────

    #[test]
    fn parse_rspec_verbose_documentation_format() {
        let output = r#"
User authentication
  with valid credentials
    allows login (0.02s)
    redirects to dashboard (0.01s)
  with invalid credentials
    shows error message (0.01s)
    increments attempt counter (FAILED - 1)

Finished in 0.04 seconds
4 examples, 1 failure
"#;
        let suites = parse_rspec_verbose(output);
        assert!(!suites.is_empty());

        let all_tests: Vec<_> = suites.iter().flat_map(|s| &s.tests).collect();
        assert!(all_tests.len() >= 4);

        let failed: Vec<_> = all_tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].name.contains("increments attempt counter"));
    }

    #[test]
    fn parse_rspec_verbose_with_pending() {
        let output = r#"
Calculator
  adds numbers (0.01s)
  subtracts (PENDING: Not yet implemented)
  multiplies (0.00s)
"#;
        let suites = parse_rspec_verbose(output);
        let all_tests: Vec<_> = suites.iter().flat_map(|s| &s.tests).collect();

        let pending: Vec<_> = all_tests
            .iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .collect();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn parse_rspec_inline_duration_parsing() {
        assert_eq!(
            parse_rspec_inline_duration("0.02s)"),
            Duration::from_millis(20)
        );
        assert_eq!(
            parse_rspec_inline_duration("1.5 seconds)"),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn is_rspec_test_line_detection() {
        assert!(is_rspec_test_line("allows login (0.02s)"));
        assert!(is_rspec_test_line("fails (FAILED - 1)"));
        assert!(is_rspec_test_line("is pending (PENDING: reason)"));
        assert!(!is_rspec_test_line("User authentication"));
        assert!(!is_rspec_test_line("with valid credentials"));
    }

    // ─── Verbose Minitest Tests ─────────────────────────────────────────

    #[test]
    fn parse_minitest_verbose_output() {
        let output = r#"
TestUser#test_name_returns_full_name = 0.01 s = .
TestUser#test_email_validation = 0.00 s = F
TestUser#test_age_is_positive = 0.00 s = S
TestCalc#test_add = 0.01 s = .
TestCalc#test_divide = 0.00 s = E
"#;
        let suites = parse_minitest_verbose(output);
        assert_eq!(suites.len(), 2);

        let user_suite = suites.iter().find(|s| s.name == "TestUser").unwrap();
        assert_eq!(user_suite.tests.len(), 3);
        assert_eq!(user_suite.tests[0].status, TestStatus::Passed);
        assert_eq!(user_suite.tests[1].status, TestStatus::Failed);
        assert_eq!(user_suite.tests[2].status, TestStatus::Skipped);

        let calc_suite = suites.iter().find(|s| s.name == "TestCalc").unwrap();
        assert_eq!(calc_suite.tests.len(), 2);
    }

    #[test]
    fn parse_minitest_verbose_result_dot() {
        let (dur, status) = parse_minitest_verbose_result("0.01 s = .");
        assert_eq!(status, TestStatus::Passed);
        assert!(dur.as_millis() >= 10);
    }

    #[test]
    fn parse_minitest_verbose_result_fail() {
        let (_, status) = parse_minitest_verbose_result("0.00 s = F");
        assert_eq!(status, TestStatus::Failed);
    }

    #[test]
    fn parse_minitest_verbose_result_error() {
        let (_, status) = parse_minitest_verbose_result("0.00 s = E");
        assert_eq!(status, TestStatus::Failed);
    }

    #[test]
    fn parse_minitest_verbose_result_skip() {
        let (_, status) = parse_minitest_verbose_result("0.00 s = S");
        assert_eq!(status, TestStatus::Skipped);
    }

    // ─── RSpec Failure Extraction Tests ──────────────────────────────────

    #[test]
    fn parse_rspec_failure_blocks() {
        let output = r#"
Failures:

  1) Calculator adds two numbers
     Failure/Error: expect(sum).to eq(5)

       expected: 5
            got: 4

     # ./spec/calculator_spec.rb:25:in `block (3 levels)'

  2) User validates email
     Failure/Error: expect(user).to be_valid

       expected valid? to return true, got false

     # ./spec/user_spec.rb:12:in `block (2 levels)'

Finished in 0.05 seconds
"#;
        let failures = parse_rspec_failures(output);
        assert_eq!(failures.len(), 2);

        assert_eq!(failures[0].name, "Calculator adds two numbers");
        assert!(failures[0].message.contains("expected: 5"));
        assert!(
            failures[0]
                .location
                .as_ref()
                .unwrap()
                .contains("calculator_spec.rb:25")
        );

        assert_eq!(failures[1].name, "User validates email");
        assert!(failures[1].message.contains("expected valid?"));
    }

    #[test]
    fn parse_rspec_failures_empty() {
        let output = "Finished in 0.01 seconds\n3 examples, 0 failures\n";
        let failures = parse_rspec_failures(output);
        assert!(failures.is_empty());
    }

    // ─── Minitest Failure Extraction Tests ───────────────────────────────

    #[test]
    fn parse_minitest_failure_blocks() {
        let output = r#"
  1) Failure:
TestUser#test_email_validation [test/user_test.rb:15]:
Expected: true
  Actual: false

  2) Error:
TestCalc#test_divide [test/calc_test.rb:8]:
ZeroDivisionError: divided by 0
"#;
        let failures = parse_minitest_failures(output);
        assert_eq!(failures.len(), 2);

        assert_eq!(failures[0].name, "test_email_validation");
        assert!(failures[0].message.contains("Expected: true"));
        assert_eq!(
            failures[0].location.as_ref().unwrap(),
            "test/user_test.rb:15"
        );

        assert_eq!(failures[1].name, "test_divide");
        assert!(failures[1].message.contains("ZeroDivisionError"));
    }

    #[test]
    fn parse_minitest_failures_empty() {
        let output = "4 runs, 4 assertions, 0 failures, 0 errors, 0 skips\n";
        let failures = parse_minitest_failures(output);
        assert!(failures.is_empty());
    }

    // ─── Error Enrichment Tests ─────────────────────────────────────────

    #[test]
    fn enrich_tests_with_rspec_errors() {
        let suites = vec![TestSuite {
            name: "spec".into(),
            tests: vec![
                TestCase {
                    name: "adds two numbers".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "subtracts".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(10),
                    error: None,
                },
            ],
        }];

        let rspec_failures = vec![RspecFailure {
            name: "Calculator adds two numbers".to_string(),
            message: "expected: 5\n     got: 4".to_string(),
            location: Some("./spec/calc_spec.rb:10".to_string()),
        }];

        let enriched = enrich_with_errors(suites, &rspec_failures, &[]);
        let failed = &enriched[0].tests[0];
        assert!(failed.error.is_some());
        let err = failed.error.as_ref().unwrap();
        assert!(err.message.contains("expected: 5"));
        assert!(err.location.as_ref().unwrap().contains("calc_spec.rb"));
    }

    #[test]
    fn enrich_tests_with_minitest_errors() {
        let suites = vec![TestSuite {
            name: "tests".into(),
            tests: vec![TestCase {
                name: "test_email_validation".into(),
                status: TestStatus::Failed,
                duration: Duration::from_millis(0),
                error: None,
            }],
        }];

        let minitest_failures = vec![MinitestFailure {
            name: "test_email_validation".to_string(),
            message: "Expected: true\n  Actual: false".to_string(),
            location: Some("test/user_test.rb:15".to_string()),
        }];

        let enriched = enrich_with_errors(suites, &[], &minitest_failures);
        let failed = &enriched[0].tests[0];
        assert!(failed.error.is_some());
    }

    #[test]
    fn truncate_message_short() {
        assert_eq!(truncate_message("hello", 10), "hello");
    }

    #[test]
    fn truncate_message_long() {
        let long = "a".repeat(600);
        let result = truncate_message(&long, 500);
        assert_eq!(result.len(), 503); // 500 + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn strip_failure_number_valid() {
        assert_eq!(
            strip_failure_number("1) Calculator adds two numbers"),
            Some("Calculator adds two numbers")
        );
    }

    #[test]
    fn strip_failure_number_double_digit() {
        assert_eq!(
            strip_failure_number("12) Some test name"),
            Some("Some test name")
        );
    }

    #[test]
    fn strip_failure_number_invalid() {
        assert_eq!(strip_failure_number("not a number"), None);
    }

    // ─── Integration Tests ──────────────────────────────────────────────

    #[test]
    fn full_rspec_verbose_with_failures() {
        let stdout = r#"
User authentication
  with valid credentials
    allows login (0.02s)
  with invalid credentials
    shows error message (FAILED - 1)

Failures:

  1) User authentication with invalid credentials shows error message
     Failure/Error: expect(page).to have_content("Error")

       expected to find text "Error" in "Welcome"

     # ./spec/auth_spec.rb:25:in `block (3 levels)'

Finished in 0.03 seconds (files took 0.1 seconds to load)
2 examples, 1 failure
"#;
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        // Should have parsed verbose tests
        let all_tests: Vec<_> = result.suites.iter().flat_map(|s| &s.tests).collect();
        assert!(all_tests.len() >= 2);

        // Failed test should have error details
        let failed: Vec<_> = all_tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .collect();
        assert!(!failed.is_empty());
    }

    #[test]
    fn full_minitest_verbose_with_failures() {
        let stdout = r#"
TestUser#test_name_returns_full_name = 0.01 s = .
TestUser#test_email_validation = 0.00 s = F

  1) Failure:
TestUser#test_email_validation [test/user_test.rb:15]:
Expected: true
  Actual: false

4 runs, 4 assertions, 1 failures, 0 errors, 0 skips
"#;
        let adapter = RubyAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        let all_tests: Vec<_> = result.suites.iter().flat_map(|s| &s.tests).collect();
        assert!(!all_tests.is_empty());
    }

    #[test]
    fn detect_minitest_via_test_dir() {
        let dir = tempfile::tempdir().unwrap();
        let test_dir = dir.path().join("test");
        std::fs::create_dir(&test_dir).unwrap();
        std::fs::write(test_dir.join("test_example.rb"), "# test").unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "minitest");
    }

    #[test]
    fn detect_no_ruby_from_bare_test_dir() {
        // A bare test/ directory without .rb files should NOT trigger Ruby detection
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("test")).unwrap();
        let adapter = RubyAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn detect_minitest_via_rakefile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Rakefile"), "require 'rake/testtask'\n").unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "minitest");
    }

    #[test]
    fn detect_rspec_via_spec_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("spec")).unwrap();
        let adapter = RubyAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "rspec");
    }
}
