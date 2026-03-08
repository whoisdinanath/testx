use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

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

        // test/ directory exists
        if project_dir.join("test").is_dir() {
            return Some("minitest");
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

        Some(DetectionResult {
            language: "Ruby".into(),
            framework: framework.into(),
            confidence: 0.9,
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

        let suites = if combined.contains("example") || combined.contains("Example") {
            parse_rspec_output(&combined, exit_code)
        } else {
            parse_minitest_output(&combined, exit_code)
        };

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
                return Some(Duration::from_secs_f64(secs));
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
}
