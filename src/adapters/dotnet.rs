use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct DotnetAdapter;

impl Default for DotnetAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DotnetAdapter {
    pub fn new() -> Self {
        Self
    }

    fn has_dotnet_project(project_dir: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(".csproj") || name.ends_with(".fsproj") || name.ends_with(".sln")
                {
                    return true;
                }
            }
        }
        false
    }

    fn detect_project_type(project_dir: &Path) -> &'static str {
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(".fsproj") {
                    return "F#";
                }
            }
        }
        "C#"
    }
}

impl TestAdapter for DotnetAdapter {
    fn name(&self) -> &str {
        "C#/.NET"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("dotnet").is_err() {
            return Some("dotnet not found. Install .NET SDK.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        if !Self::has_dotnet_project(project_dir) {
            return None;
        }

        let lang = Self::detect_project_type(project_dir);

        Some(DetectionResult {
            language: lang.into(),
            framework: "dotnet test".into(),
            confidence: 0.95,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("dotnet");
        cmd.arg("test");
        cmd.arg("--verbosity");
        cmd.arg("normal");

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = format!("{}\n{}", stdout, stderr);

        let suites = parse_dotnet_output(&combined, exit_code);
        let duration = parse_dotnet_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse dotnet test output.
///
/// Format:
/// ```text
/// Starting test execution, please wait...
/// A total of 1 test files matched the specified pattern.
///
///   Passed test_add [< 1 ms]
///   Passed test_subtract [2 ms]
///   Failed test_divide [< 1 ms]
///     Error Message:
///       Assert.Equal() Failure
///
/// Test Run Successful.
/// Total tests: 3
///      Passed: 2
///      Failed: 1
///     Skipped: 0
/// ```
fn parse_dotnet_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();
    let mut found_summary = false;

    // Try detailed output first ("  Passed test_name [duration]")
    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Passed ")
            || trimmed.starts_with("Failed ")
            || trimmed.starts_with("Skipped ")
        {
            let (status, rest) = if let Some(rest) = trimmed.strip_prefix("Passed ") {
                (TestStatus::Passed, rest)
            } else if let Some(rest) = trimmed.strip_prefix("Failed ") {
                (TestStatus::Failed, rest)
            } else if let Some(rest) = trimmed.strip_prefix("Skipped ") {
                (TestStatus::Skipped, rest)
            } else {
                continue;
            };

            // Name might have " [duration]" suffix
            let name = rest
                .rfind('[')
                .map(|i| rest[..i].trim())
                .unwrap_or(rest)
                .to_string();

            // Parse duration from "[2 ms]" or "[< 1 ms]"
            let duration = if let Some(bracket_start) = rest.rfind('[') {
                let dur_str = &rest[bracket_start + 1..rest.len().saturating_sub(1)];
                parse_dotnet_test_duration(dur_str)
            } else {
                Duration::from_millis(0)
            };

            tests.push(TestCase {
                name,
                status,
                duration,
                error: None,
            });
        }
    }

    // Fallback: parse summary section
    if tests.is_empty() {
        let mut total = 0usize;
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        for line in output.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("Total tests:") {
                total = rest.trim().parse().unwrap_or(0);
                found_summary = true;
            } else if let Some(rest) = trimmed.strip_prefix("Passed:") {
                passed = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = trimmed.strip_prefix("Failed:") {
                failed = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = trimmed.strip_prefix("Skipped:") {
                skipped = rest.trim().parse().unwrap_or(0);
            }
        }

        if found_summary && total > 0 {
            // Use parsed counts; if passed wasn't explicitly listed, calculate it
            if passed == 0 && failed + skipped < total {
                passed = total - failed - skipped;
            }
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

fn parse_dotnet_test_duration(dur_str: &str) -> Duration {
    // "2 ms", "< 1 ms", "1.5 s"
    let clean = dur_str.trim().trim_start_matches("< ");
    let parts: Vec<&str> = clean.split_whitespace().collect();
    if parts.len() >= 2 {
        let value: f64 = parts[0].parse().unwrap_or(0.0);
        match parts[1] {
            "ms" => Duration::from_secs_f64(value / 1000.0),
            "s" => Duration::from_secs_f64(value),
            _ => Duration::from_millis(0),
        }
    } else {
        Duration::from_millis(0)
    }
}

fn parse_dotnet_duration(output: &str) -> Option<Duration> {
    // "Total time: 1.234 Seconds" or "Duration: 1.234 s"
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Total time:") || trimmed.starts_with("Duration:") {
            let num_str: String = trimmed
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
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
    fn detect_csproj() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MyApp.csproj"), "<Project/>").unwrap();
        let adapter = DotnetAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "C#");
        assert_eq!(det.framework, "dotnet test");
    }

    #[test]
    fn detect_fsproj() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MyApp.fsproj"), "<Project/>").unwrap();
        let adapter = DotnetAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "F#");
    }

    #[test]
    fn detect_sln() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MyApp.sln"), "").unwrap();
        let adapter = DotnetAdapter::new();
        assert!(adapter.detect(dir.path()).is_some());
    }

    #[test]
    fn detect_no_dotnet() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = DotnetAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_dotnet_detailed_output() {
        let stdout = r#"
Starting test execution, please wait...
A total of 1 test files matched the specified pattern.

  Passed test_add [2 ms]
  Passed test_subtract [< 1 ms]
  Failed test_divide [3 ms]

Test Run Failed.
Total tests: 3
     Passed: 2
     Failed: 1
    Skipped: 0
"#;
        let adapter = DotnetAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_dotnet_all_pass() {
        let stdout = r#"
  Passed test_add [2 ms]
  Passed test_subtract [1 ms]

Test Run Successful.
Total tests: 2
     Passed: 2
     Failed: 0
    Skipped: 0
"#;
        let adapter = DotnetAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_dotnet_summary_only() {
        let stdout = r#"
Test Run Successful.
Total tests: 5
     Passed: 4
     Failed: 0
    Skipped: 1
"#;
        let adapter = DotnetAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 4);
        assert_eq!(result.total_skipped(), 1);
    }

    #[test]
    fn parse_dotnet_empty_output() {
        let adapter = DotnetAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_test_duration_ms() {
        assert_eq!(parse_dotnet_test_duration("2 ms"), Duration::from_millis(2));
    }

    #[test]
    fn parse_test_duration_lt_ms() {
        assert_eq!(
            parse_dotnet_test_duration("< 1 ms"),
            Duration::from_millis(1)
        );
    }
}
