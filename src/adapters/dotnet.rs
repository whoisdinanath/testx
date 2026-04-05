use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::{combined_output, duration_from_secs_safe, has_marker_in_subdirs, truncate};
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus,
    TestSuite,
};

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
        // Check root directory first
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
        // Fallback: check up to 2 levels deep (e.g., Src/Project/Project.csproj)
        has_marker_in_subdirs(project_dir, 2, |name| {
            name.ends_with(".csproj") || name.ends_with(".fsproj") || name.ends_with(".sln")
        })
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
        // Also check subdirectories for F# projects
        if has_marker_in_subdirs(project_dir, 2, |name| name.ends_with(".fsproj")) {
            return "F#";
        }
        "C#"
    }

    /// Find the best project/solution file to pass to `dotnet test`.
    ///
    /// Priority: .sln at root > .sln in subdirs > .csproj/.fsproj at root > in subdirs.
    /// Returns None if a project file exists at root (dotnet discovers it automatically).
    fn find_project_file(project_dir: &Path) -> Option<std::path::PathBuf> {
        // If root has .sln or .csproj/.fsproj, dotnet test auto-discovers — no path needed
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".sln")
                    || name_str.ends_with(".csproj")
                    || name_str.ends_with(".fsproj")
                {
                    return None;
                }
            }
        }

        // Search subdirectories: prefer .sln files over .csproj/.fsproj
        let mut best_sln: Option<std::path::PathBuf> = None;
        let mut best_proj: Option<std::path::PathBuf> = None;
        Self::scan_for_project_files(project_dir, 2, &mut best_sln, &mut best_proj);
        best_sln.or(best_proj)
    }

    fn scan_for_project_files(
        dir: &Path,
        depth: u8,
        best_sln: &mut Option<std::path::PathBuf>,
        best_proj: &mut Option<std::path::PathBuf>,
    ) {
        if best_sln.is_some() {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let path = entry.path();
            if path.is_file() {
                if name_str.ends_with(".sln") {
                    *best_sln = Some(path);
                    return; // .sln always wins
                }
                if name_str.ends_with(".csproj") || name_str.ends_with(".fsproj") {
                    // Prefer test projects over library projects
                    let is_test_proj = name_str.contains("Test") || name_str.contains("test");
                    if is_test_proj || best_proj.is_none() {
                        *best_proj = Some(path);
                    }
                }
            } else if depth > 0 && path.is_dir() && !name_str.starts_with('.') {
                let skip = matches!(
                    name_str.as_ref(),
                    "node_modules" | "vendor" | "target" | "bin" | "obj" | "packages"
                );
                if !skip {
                    Self::scan_for_project_files(&path, depth - 1, best_sln, best_proj);
                }
            }
        }
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

        // Check for .sln at root or in subdirectories
        let has_sln = std::fs::read_dir(project_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().ends_with(".sln"))
            })
            .unwrap_or(false)
            || has_marker_in_subdirs(project_dir, 2, |name| name.ends_with(".sln"));

        // Check for build artifacts at root or in subdirs
        let has_build_artifacts = project_dir.join("obj").is_dir()
            || project_dir.join("bin").is_dir()
            || has_marker_in_subdirs(project_dir, 2, |name| name == "obj" || name == "bin");

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, has_sln)
            .signal(0.10, has_build_artifacts)
            .signal(0.15, which::which("dotnet").is_ok())
            .finish();

        Some(DetectionResult {
            language: lang.into(),
            framework: "dotnet test".into(),
            confidence,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let mut cmd = Command::new("dotnet");
        cmd.arg("test");

        // If project files are in subdirectories, pass the path explicitly
        if let Some(project_file) = Self::find_project_file(project_dir) {
            cmd.arg(&project_file);
        }

        cmd.arg("--verbosity");
        cmd.arg("normal");

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

        let mut suites = parse_dotnet_output(&combined, exit_code);

        // Enrich failed tests with error details from stack traces
        let failures = parse_dotnet_failures(&combined);
        if !failures.is_empty() {
            enrich_with_errors(&mut suites, &failures);
        }

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
///
/// Check if a line is a real dotnet test result (not MSBuild noise).
///
/// Real test results look like:
///   `Passed Some.Test.Name [18 ms]`
///   `Failed Some.Test.Name [< 1 ms]`
///   `Skipped Some.Test.Name`
///
/// MSBuild noise looks like:
///   `Failed to load prune package data from PrunePackageData folder, ...`
///
/// We require either a `[duration]` suffix or that the rest after the
/// status prefix contains a dotted name (Namespace.Class.Method).
fn is_dotnet_test_result_line(trimmed: &str) -> bool {
    let rest = if let Some(r) = trimmed.strip_prefix("Passed ") {
        r
    } else if let Some(r) = trimmed.strip_prefix("Failed ") {
        r
    } else if let Some(r) = trimmed.strip_prefix("Skipped ") {
        r
    } else {
        return false;
    };

    // Has a [duration] suffix — definitely a test result
    if rest.ends_with(']') && rest.contains('[') {
        return true;
    }

    // Has a dotted name like Namespace.Class.Method — test result
    let name_part = rest.split_whitespace().next().unwrap_or("");
    if name_part.contains('.') && !name_part.starts_with('.') {
        return true;
    }

    false
}

fn parse_dotnet_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();
    let mut found_summary = false;

    // Try detailed output first ("  Passed test_name [duration]")
    for line in output.lines() {
        let trimmed = line.trim();

        if !is_dotnet_test_result_line(trimmed) {
            // Skip non-test lines (MSBuild noise, etc.)
        } else if trimmed.starts_with("Passed ")
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
            "ms" => duration_from_secs_safe(value / 1000.0),
            "s" => duration_from_secs_safe(value),
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
                return Some(duration_from_secs_safe(secs));
            }
        }
    }
    None
}

/// A parsed failure from dotnet test output.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DotnetFailure {
    /// Test name
    test_name: String,
    /// Error/assertion message
    message: String,
    /// Stack trace lines
    stack_trace: Option<String>,
    /// File location extracted from stack trace
    location: Option<String>,
}

/// Parse dotnet test failure blocks.
///
/// Format:
/// ```text
///   Failed test_divide [< 1 ms]
///   Error Message:
///    Assert.Equal() Failure
///    Expected: 4
///    Actual:   3
///   Stack Trace:
///    at MyApp.Tests.MathTest.TestDivide() in /path/MathTest.cs:line 42
/// ```
///
/// Or xUnit/NUnit format:
/// ```text
///   X test_divide [< 1 ms]
///     Error Message:
///       Assert.Equal() Failure
///     Stack Trace:
///       at MyApp.Tests.MathTest.TestDivide() in /tests/MathTest.cs:line 42
/// ```
fn parse_dotnet_failures(output: &str) -> Vec<DotnetFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Find "Failed test_name [duration]" lines (skip MSBuild noise)
        let is_failed_test = if trimmed.starts_with("Failed ") {
            is_dotnet_test_result_line(trimmed)
        } else {
            trimmed.starts_with("X ")
        };
        if is_failed_test {
            let rest = if let Some(r) = trimmed.strip_prefix("Failed ") {
                r
            } else if let Some(r) = trimmed.strip_prefix("X ") {
                r
            } else {
                i += 1;
                continue;
            };

            // Extract test name (before the duration bracket)
            let test_name = rest
                .rfind('[')
                .map(|idx| rest[..idx].trim())
                .unwrap_or(rest)
                .to_string();

            i += 1;

            // Collect error message and stack trace
            let mut message_lines = Vec::new();
            let mut stack_lines = Vec::new();
            let mut in_message = false;
            let mut in_stack = false;

            while i < lines.len() {
                let line = lines[i].trim();

                // Detect section headers
                if line == "Error Message:" || line.starts_with("Error Message:") {
                    in_message = true;
                    in_stack = false;
                    i += 1;
                    continue;
                }
                if line == "Stack Trace:" || line.starts_with("Stack Trace:") {
                    in_message = false;
                    in_stack = true;
                    i += 1;
                    continue;
                }

                // Stop at next test result or empty context
                if is_dotnet_test_result_line(line)
                    || line.starts_with("X ")
                    || line.starts_with("Test Run")
                    || line.starts_with("Total tests:")
                {
                    break;
                }

                if in_message && !line.is_empty() {
                    message_lines.push(line.to_string());
                } else if in_stack && !line.is_empty() {
                    stack_lines.push(line.to_string());
                }

                i += 1;
            }

            let message = if message_lines.is_empty() {
                "Test failed".to_string()
            } else {
                truncate(&message_lines.join("\n"), 500)
            };

            let stack_trace = if stack_lines.is_empty() {
                None
            } else {
                Some(
                    stack_lines
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            };

            let location = stack_lines.iter().find_map(|l| extract_dotnet_location(l));

            failures.push(DotnetFailure {
                test_name,
                message,
                stack_trace,
                location,
            });
            continue;
        }

        i += 1;
    }

    failures
}

/// Extract file location from a .NET stack trace line.
/// "at Namespace.Class.Method() in /path/File.cs:line 42"
fn extract_dotnet_location(line: &str) -> Option<String> {
    // Look for " in " followed by a path and ":line N"
    if let Some(in_idx) = line.find(" in ") {
        let path_part = &line[in_idx + 4..];
        let path = path_part.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    // Direct file:line pattern
    if (line.contains(".cs:") || line.contains(".fs:")) && line.contains("line ") {
        return Some(line.trim().to_string());
    }
    None
}

/// Enrich test cases with failure details.
fn enrich_with_errors(suites: &mut [TestSuite], failures: &[DotnetFailure]) {
    for suite in suites.iter_mut() {
        for test in suite.tests.iter_mut() {
            if test.status != TestStatus::Failed || test.error.is_some() {
                continue;
            }
            if let Some(failure) = find_matching_dotnet_failure(&test.name, failures) {
                test.error = Some(TestError {
                    message: failure.message.clone(),
                    location: failure.location.clone(),
                });
            }
        }
    }
}

/// Find a matching failure for a test name.
fn find_matching_dotnet_failure<'a>(
    test_name: &str,
    failures: &'a [DotnetFailure],
) -> Option<&'a DotnetFailure> {
    for failure in failures {
        if failure.test_name == test_name {
            return Some(failure);
        }
        // Partial match: test name might be namespace-qualified
        if failure.test_name.ends_with(test_name) || test_name.ends_with(&failure.test_name) {
            return Some(failure);
        }
    }
    if failures.len() == 1 {
        return Some(&failures[0]);
    }
    None
}

/// Parse dotnet test TRX report files.
///
/// TRX files are XML test result files generated by `dotnet test --logger trx`.
/// Located at: TestResults/*.trx
pub fn parse_trx_report(project_dir: &Path) -> Vec<TestSuite> {
    let results_dir = project_dir.join("TestResults");
    if !results_dir.is_dir() {
        return Vec::new();
    }

    let mut suites = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&results_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".trx")
                && let Ok(content) = std::fs::read_to_string(entry.path())
            {
                let mut parsed = parse_trx_content(&content);
                suites.append(&mut parsed);
            }
        }
    }

    suites
}

/// Parse TRX XML content.
///
/// TRX format (simplified):
/// ```xml
/// <TestRun>
///   <Results>
///     <UnitTestResult testName="TestAdd" outcome="Passed" duration="00:00:00.001">
///     </UnitTestResult>
///     <UnitTestResult testName="TestDiv" outcome="Failed" duration="00:00:00.002">
///       <Output>
///         <ErrorInfo>
///           <Message>Assert.Equal failure</Message>
///           <StackTrace>at Test.TestDiv() in Test.cs:line 42</StackTrace>
///         </ErrorInfo>
///       </Output>
///     </UnitTestResult>
///   </Results>
/// </TestRun>
/// ```
fn parse_trx_content(content: &str) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    // Find all <UnitTestResult> elements
    let mut search_from = 0;

    while let Some(start) = content[search_from..].find("<UnitTestResult") {
        let abs_start = search_from + start;

        // Find end of this element
        let end = if let Some(close) = content[abs_start..].find("</UnitTestResult>") {
            abs_start + close + 17
        } else if let Some(self_close) = content[abs_start..].find("/>") {
            abs_start + self_close + 2
        } else {
            break;
        };

        let element = &content[abs_start..end];

        let test_name = extract_trx_attr(element, "testName").unwrap_or_else(|| "unknown".into());
        let outcome = extract_trx_attr(element, "outcome").unwrap_or_default();
        let duration_str = extract_trx_attr(element, "duration").unwrap_or_default();

        let status = match outcome.as_str() {
            "Passed" => TestStatus::Passed,
            "Failed" => TestStatus::Failed,
            "NotExecuted" | "Inconclusive" => TestStatus::Skipped,
            _ => TestStatus::Failed,
        };

        let duration = parse_trx_duration(&duration_str);

        let error = if status == TestStatus::Failed {
            let message =
                extract_trx_error_message(element).unwrap_or_else(|| "Test failed".into());
            let location =
                extract_trx_stack_trace(element).and_then(|st| extract_dotnet_location(&st));
            Some(TestError { message, location })
        } else {
            None
        };

        tests.push(TestCase {
            name: test_name,
            status,
            duration,
            error,
        });

        search_from = end;
    }

    if tests.is_empty() {
        return Vec::new();
    }

    vec![TestSuite {
        name: "tests".into(),
        tests,
    }]
}

/// Extract an attribute from a TRX element.
fn extract_trx_attr(element: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = element.find(&pattern)?;
    let value_start = start + pattern.len();
    let value_end = element[value_start..].find('"')?;
    Some(element[value_start..value_start + value_end].to_string())
}

/// Parse TRX duration format: "00:00:00.001"
fn parse_trx_duration(s: &str) -> Duration {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().unwrap_or(0.0);
        let mins: f64 = parts[1].parse().unwrap_or(0.0);
        let secs: f64 = parts[2].parse().unwrap_or(0.0);
        duration_from_secs_safe(hours * 3600.0 + mins * 60.0 + secs)
    } else {
        Duration::from_millis(0)
    }
}

/// Extract error message from TRX <ErrorInfo><Message> element.
fn extract_trx_error_message(element: &str) -> Option<String> {
    let msg_start = element.find("<Message>")?;
    let msg_end = element[msg_start..].find("</Message>")?;
    let message = &element[msg_start + 9..msg_start + msg_end];
    Some(message.trim().to_string())
}

/// Extract stack trace from TRX <ErrorInfo><StackTrace> element.
fn extract_trx_stack_trace(element: &str) -> Option<String> {
    let st_start = element.find("<StackTrace>")?;
    let st_end = element[st_start..].find("</StackTrace>")?;
    let trace = &element[st_start + 12..st_start + st_end];
    Some(trace.trim().to_string())
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

    #[test]
    fn parse_dotnet_failure_blocks() {
        let output = r#"
  Passed test_add [2 ms]
  Failed test_divide [< 1 ms]
  Error Message:
   Assert.Equal() Failure
   Expected: 4
   Actual:   3
  Stack Trace:
   at MyApp.Tests.MathTest.TestDivide() in /tests/MathTest.cs:line 42

Test Run Failed.
Total tests: 2
     Passed: 1
     Failed: 1
"#;
        let failures = parse_dotnet_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].test_name, "test_divide");
        assert!(failures[0].message.contains("Assert.Equal"));
        assert!(failures[0].location.is_some());
        assert!(
            failures[0]
                .location
                .as_ref()
                .unwrap()
                .contains("MathTest.cs:line 42")
        );
    }

    #[test]
    fn parse_dotnet_multiple_failures() {
        let output = r#"
  Failed test_a [1 ms]
  Error Message:
   Expected True but got False
  Stack Trace:
   at Tests.A() in /tests/Test.cs:line 10

  Failed test_b [2 ms]
  Error Message:
   Null reference
  Stack Trace:
   at Tests.B() in /tests/Test.cs:line 20

Test Run Failed.
"#;
        let failures = parse_dotnet_failures(output);
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].test_name, "test_a");
        assert_eq!(failures[1].test_name, "test_b");
    }

    #[test]
    fn parse_dotnet_failure_no_stack() {
        let output = r#"
  Failed test_x [1 ms]
  Error Message:
   Something went wrong

  Passed test_y [1 ms]
"#;
        let failures = parse_dotnet_failures(output);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].stack_trace.is_none());
    }

    #[test]
    fn extract_dotnet_location_test() {
        assert_eq!(
            extract_dotnet_location(
                "at MyApp.Tests.MathTest.TestDivide() in /tests/MathTest.cs:line 42"
            ),
            Some("/tests/MathTest.cs:line 42".into())
        );
        assert!(extract_dotnet_location("no location here").is_none());
    }

    #[test]
    fn enrich_with_errors_test() {
        let mut suites = vec![TestSuite {
            name: "tests".into(),
            tests: vec![
                TestCase {
                    name: "test_add".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "test_divide".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
            ],
        }];
        let failures = vec![DotnetFailure {
            test_name: "test_divide".into(),
            message: "Assert.Equal failure".into(),
            stack_trace: Some("at Test.TestDivide() in /tests/Test.cs:line 42".into()),
            location: Some("/tests/Test.cs:line 42".into()),
        }];
        enrich_with_errors(&mut suites, &failures);
        assert!(suites[0].tests[0].error.is_none());
        let err = suites[0].tests[1].error.as_ref().unwrap();
        assert_eq!(err.message, "Assert.Equal failure");
        assert!(err.location.as_ref().unwrap().contains("Test.cs:line 42"));
    }

    #[test]
    fn truncate_test() {
        assert_eq!(truncate("short", 100), "short");
        let long = "m".repeat(600);
        let truncated = truncate(&long, 500);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn parse_trx_basic() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<TestRun>
  <Results>
    <UnitTestResult testName="TestAdd" outcome="Passed" duration="00:00:00.001">
    </UnitTestResult>
    <UnitTestResult testName="TestDiv" outcome="Failed" duration="00:00:00.002">
      <Output>
        <ErrorInfo>
          <Message>Assert.Equal failure</Message>
          <StackTrace>at Test.TestDiv() in /tests/Test.cs:line 42</StackTrace>
        </ErrorInfo>
      </Output>
    </UnitTestResult>
  </Results>
</TestRun>"#;
        let suites = parse_trx_content(content);
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].tests.len(), 2);
        assert_eq!(suites[0].tests[0].name, "TestAdd");
        assert_eq!(suites[0].tests[0].status, TestStatus::Passed);
        assert_eq!(suites[0].tests[1].name, "TestDiv");
        assert_eq!(suites[0].tests[1].status, TestStatus::Failed);
        assert!(suites[0].tests[1].error.is_some());
    }

    #[test]
    fn parse_trx_skipped() {
        let content = r#"<TestRun><Results>
<UnitTestResult testName="TestSkip" outcome="NotExecuted" duration="00:00:00.000"/>
</Results></TestRun>"#;
        let suites = parse_trx_content(content);
        assert_eq!(suites[0].tests[0].status, TestStatus::Skipped);
    }

    #[test]
    fn parse_trx_duration_test() {
        assert_eq!(
            parse_trx_duration("00:00:01.500"),
            Duration::from_millis(1500)
        );
        assert_eq!(parse_trx_duration("00:01:00.000"), Duration::from_secs(60));
    }

    #[test]
    fn extract_trx_attr_test() {
        assert_eq!(
            extract_trx_attr(
                r#"<UnitTestResult testName="TestAdd" outcome="Passed">"#,
                "testName"
            ),
            Some("TestAdd".into())
        );
        assert_eq!(
            extract_trx_attr(
                r#"<UnitTestResult testName="TestAdd" outcome="Passed">"#,
                "outcome"
            ),
            Some("Passed".into())
        );
    }

    #[test]
    fn extract_trx_error_message_test() {
        let element =
            "<Output><ErrorInfo><Message>Assert.Equal failure</Message></ErrorInfo></Output>";
        assert_eq!(
            extract_trx_error_message(element),
            Some("Assert.Equal failure".into())
        );
    }

    #[test]
    fn extract_trx_stack_trace_test() {
        let element = "<Output><ErrorInfo><StackTrace>at Test.Run() in Test.cs:line 10</StackTrace></ErrorInfo></Output>";
        assert_eq!(
            extract_trx_stack_trace(element),
            Some("at Test.Run() in Test.cs:line 10".into())
        );
    }

    #[test]
    fn parse_dotnet_failure_integration() {
        let stdout = r#"
Starting test execution, please wait...

  Passed test_add [2 ms]
  Failed test_divide [< 1 ms]
  Error Message:
   Assert.Equal() Failure
   Expected: 4
   Actual:   3
  Stack Trace:
   at Tests.Divide() in /tests/MathTest.cs:line 42

Test Run Failed.
Total tests: 2
     Passed: 1
     Failed: 1
"#;
        let adapter = DotnetAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
        let failed = result.suites[0]
            .tests
            .iter()
            .find(|t| t.status == TestStatus::Failed)
            .unwrap();
        assert!(failed.error.is_some());
        assert!(
            failed
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("Assert.Equal")
        );
    }
}
