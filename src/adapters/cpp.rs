use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{
    DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus, TestSuite,
};

pub struct CppAdapter;

impl Default for CppAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CppAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Detect build system: CMake (ctest) or Meson
    fn detect_build_system(project_dir: &Path) -> Option<&'static str> {
        let cmake = project_dir.join("CMakeLists.txt");
        if cmake.exists() {
            return Some("cmake");
        }
        if project_dir.join("meson.build").exists() {
            return Some("meson");
        }
        None
    }

    /// Check if a CMake build directory exists
    fn find_build_dir(project_dir: &Path) -> Option<std::path::PathBuf> {
        for name in &[
            "build",
            "cmake-build-debug",
            "cmake-build-release",
            "out/build",
        ] {
            let p = project_dir.join(name);
            if p.is_dir() {
                return Some(p);
            }
        }
        None
    }
}

impl TestAdapter for CppAdapter {
    fn name(&self) -> &str {
        "C/C++"
    }

    fn check_runner(&self) -> Option<String> {
        if which::which("ctest").is_err() && which::which("meson").is_err() {
            return Some("ctest or meson not found. Install CMake or Meson.".into());
        }
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let build_system = Self::detect_build_system(project_dir)?;

        let framework = match build_system {
            "cmake" => "ctest",
            "meson" => "meson test",
            _ => "unknown",
        };

        Some(DetectionResult {
            language: "C/C++".into(),
            framework: framework.into(),
            confidence: 0.85,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let build_system = Self::detect_build_system(project_dir).unwrap_or("cmake");

        let mut cmd;

        match build_system {
            "meson" => {
                cmd = Command::new("meson");
                cmd.arg("test");
                cmd.arg("-C");
                let build_dir = Self::find_build_dir(project_dir)
                    .unwrap_or_else(|| project_dir.join("builddir"));
                cmd.arg(build_dir);
            }
            _ => {
                // CMake / ctest
                cmd = Command::new("ctest");
                cmd.arg("--output-on-failure");
                cmd.arg("--test-dir");
                let build_dir =
                    Self::find_build_dir(project_dir).unwrap_or_else(|| project_dir.join("build"));
                cmd.arg(build_dir);
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

        let mut suites = parse_ctest_output(&combined, exit_code);

        // Try to enrich failed tests with error details from --output-on-failure
        let failures = parse_ctest_failures(&combined);
        if !failures.is_empty() {
            enrich_with_errors(&mut suites, &failures);
        }

        // Also try parsing Google Test output if present
        let gtest_suites = parse_gtest_output(&combined);
        if !gtest_suites.is_empty() {
            // Google Test output is more detailed, prefer it
            suites = gtest_suites;
        }

        let duration = parse_ctest_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse CTest output.
///
/// Format:
/// ```text
/// Test project /path/to/build
///     Start 1: test_basic
/// 1/3 Test #1: test_basic ...................   Passed    0.01 sec
///     Start 2: test_advanced
/// 2/3 Test #2: test_advanced ................   Passed    0.02 sec
///     Start 3: test_edge
/// 3/3 Test #3: test_edge ....................***Failed    0.01 sec
///
/// 67% tests passed, 1 tests failed out of 3
/// ```
fn parse_ctest_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut tests = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // "1/3 Test #1: test_basic ...................   Passed    0.01 sec"
        if trimmed.contains("Test #")
            && (trimmed.contains("Passed")
                || trimmed.contains("Failed")
                || trimmed.contains("Not Run"))
        {
            let (name, status, duration) = parse_ctest_line(trimmed);
            tests.push(TestCase {
                name,
                status,
                duration,
                error: None,
            });
        }
    }

    // Fallback: parse summary line
    if tests.is_empty()
        && let Some((passed, failed)) = parse_ctest_summary(output)
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

fn parse_ctest_line(line: &str) -> (String, TestStatus, Duration) {
    // "1/3 Test #1: test_basic ...................   Passed    0.01 sec"
    let status = if line.contains("Passed") {
        TestStatus::Passed
    } else if line.contains("Not Run") {
        TestStatus::Skipped
    } else {
        TestStatus::Failed
    };

    // Extract test name: between "Test #N: " and the dots/spaces
    let name = if let Some(idx) = line.find(": ") {
        let after = &line[idx + 2..];
        // Name ends at first run of dots or multiple spaces
        let end = after
            .find(" .")
            .or_else(|| after.find("  "))
            .unwrap_or(after.len());
        after[..end].trim().to_string()
    } else {
        "unknown".into()
    };

    // Extract duration: "0.01 sec"
    let duration = if let Some(idx) = line.rfind("    ") {
        let after = line[idx..].trim();
        let num_str: String = after
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

    (name, status, duration)
}

fn parse_ctest_summary(output: &str) -> Option<(usize, usize)> {
    // "67% tests passed, 1 tests failed out of 3"
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("tests passed") && trimmed.contains("out of") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let mut failed = 0usize;
            let mut total = 0usize;
            for (i, part) in parts.iter().enumerate() {
                // Pattern: "N tests failed" — number is 2 before "failed"
                if *part == "failed" && i >= 2 {
                    failed = parts[i - 2].parse().unwrap_or(0);
                }
                if *part == "of" && i + 1 < parts.len() {
                    total = parts[i + 1].parse().unwrap_or(0);
                }
            }
            if total > 0 {
                return Some((total.saturating_sub(failed), failed));
            }
        }
    }
    None
}

fn parse_ctest_duration(output: &str) -> Option<Duration> {
    // "Total Test time (real) =   0.05 sec"
    for line in output.lines() {
        if line.contains("Total Test time")
            && let Some(idx) = line.find('=')
        {
            let after = line[idx + 1..].trim();
            let num_str: String = after
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

/// A parsed CTest failure with output-on-failure details.
#[derive(Debug, Clone)]
struct CTestFailure {
    /// Test name from the CTest output
    test_name: String,
    /// Captured output from the failing test
    output: String,
    /// First error/assertion line if detected
    error_line: Option<String>,
}

/// Parse CTest --output-on-failure blocks.
///
/// When a test fails with `--output-on-failure`, CTest prints the test's
/// stdout/stderr between markers:
/// ```text
/// 3/3 Test #3: test_edge ....................***Failed    0.01 sec
/// Output:
/// -------
/// ASSERTION FAILED: expected 4 but got 3
///   at test_edge.cpp:42
/// -------
/// ```
fn parse_ctest_failures(output: &str) -> Vec<CTestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Find failed test lines
        if trimmed.contains("Test #")
            && (trimmed.contains("***Failed") || trimmed.contains("***Exception"))
        {
            let test_name = extract_ctest_name(trimmed);

            // Look for output block following the failure
            let mut output_lines: Vec<String> = Vec::new();
            let mut error_line = None;
            i += 1;

            // Skip to "Output:" or collect indented output
            while i < lines.len() {
                let line = lines[i].trim();

                if line == "Output:" || line.starts_with("---") {
                    i += 1;
                    continue;
                }

                // Stop at next test start or summary line
                if line.contains("Test #")
                    || line.contains("tests passed")
                    || line.contains("Total Test time")
                    || (line.starts_with("Start ") && line.contains(':'))
                {
                    break;
                }

                if !line.is_empty() {
                    output_lines.push(line.to_string());

                    // Detect assertion/error lines
                    if error_line.is_none() && is_cpp_error_line(line) {
                        error_line = Some(line.to_string());
                    }
                }

                i += 1;
            }

            if !output_lines.is_empty() {
                failures.push(CTestFailure {
                    test_name: test_name.clone(),
                    output: truncate_output(&output_lines.join("\n"), 800),
                    error_line,
                });
            }
            continue;
        }

        i += 1;
    }

    failures
}

/// Extract test name from a CTest line.
fn extract_ctest_name(line: &str) -> String {
    if let Some(idx) = line.find(": ") {
        let after = &line[idx + 2..];
        let end = after
            .find(" .")
            .or_else(|| after.find("  "))
            .unwrap_or(after.len());
        after[..end].trim().to_string()
    } else {
        "unknown".into()
    }
}

/// Check if a line looks like a C/C++ error or assertion.
fn is_cpp_error_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("assert")
        || lower.contains("error:")
        || lower.contains("failure")
        || lower.contains("expected")
        || lower.contains("actual")
        || lower.contains("fatal")
        || lower.contains("segfault")
        || lower.contains("sigsegv")
        || lower.contains("sigabrt")
        || lower.contains("abort")
}

/// Truncate output to max length.
fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Enrich test cases with CTest failure details.
fn enrich_with_errors(suites: &mut [TestSuite], failures: &[CTestFailure]) {
    for suite in suites.iter_mut() {
        for test in suite.tests.iter_mut() {
            if test.status != TestStatus::Failed || test.error.is_some() {
                continue;
            }
            if let Some(failure) = find_matching_ctest_failure(&test.name, failures) {
                let message = failure
                    .error_line
                    .clone()
                    .unwrap_or_else(|| first_meaningful_line(&failure.output));
                test.error = Some(TestError {
                    message,
                    location: extract_cpp_location(&failure.output),
                });
            }
        }
    }
}

/// Find a matching CTest failure.
fn find_matching_ctest_failure<'a>(
    test_name: &str,
    failures: &'a [CTestFailure],
) -> Option<&'a CTestFailure> {
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

/// Get the first meaningful (non-empty, non-separator) line.
fn first_meaningful_line(s: &str) -> String {
    for line in s.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("---") && !trimmed.starts_with("===") {
            return trimmed.to_string();
        }
    }
    s.lines().next().unwrap_or("unknown error").to_string()
}

/// Extract a file:line location from C++ output.
fn extract_cpp_location(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        // GCC/Clang style: "file.cpp:42: error: ..."
        // GTest style: "test.cpp:42: Failure"
        if let Some(loc) = extract_file_line_location(trimmed) {
            return Some(loc);
        }
        // "at file.cpp:42" style
        if let Some(rest) = trimmed.strip_prefix("at ")
            && rest.contains(':')
            && (rest.contains(".cpp") || rest.contains(".c") || rest.contains(".h"))
        {
            return Some(rest.to_string());
        }
    }
    None
}

/// Extract file:line from a C-style error message.
fn extract_file_line_location(line: &str) -> Option<String> {
    // Pattern: "filename.ext:NUMBER:" or "filename.ext(NUMBER)"
    let extensions = [".cpp", ".cc", ".cxx", ".c", ".h", ".hpp"];
    for ext in &extensions {
        if let Some(ext_pos) = line.find(ext) {
            let after_ext = &line[ext_pos + ext.len()..];
            if let Some(colon_after) = after_ext.strip_prefix(':') {
                // "file.cpp:42: ..."
                let num_end = colon_after
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(colon_after.len());
                if num_end > 0 {
                    let end = ext_pos + ext.len() + 1 + num_end;
                    return Some(line[..end].trim().to_string());
                }
            } else if after_ext.starts_with('(') {
                // "file.cpp(42)"
                if let Some(paren_close) = after_ext.find(')') {
                    let end = ext_pos + ext.len() + paren_close + 1;
                    return Some(line[..end].trim().to_string());
                }
            }
        }
    }
    None
}

/// Parse Google Test output.
///
/// Format:
/// ```text
/// [==========] Running 3 tests from 1 test suite.
/// [----------] 3 tests from MathTest
/// [ RUN      ] MathTest.TestAdd
/// [       OK ] MathTest.TestAdd (0 ms)
/// [ RUN      ] MathTest.TestSub
/// [       OK ] MathTest.TestSub (0 ms)
/// [ RUN      ] MathTest.TestDiv
/// test_math.cpp:42: Failure
/// Expected equality of these values:
///   divide(10, 3)
///     Which is: 3
///   4
/// [  FAILED  ] MathTest.TestDiv (0 ms)
/// [----------] 3 tests from MathTest (0 ms total)
/// [==========] 3 tests from 1 test suite ran. (0 ms total)
/// [  PASSED  ] 2 tests.
/// [  FAILED  ] 1 test, listed below:
/// [  FAILED  ] MathTest.TestDiv
/// ```
fn parse_gtest_output(output: &str) -> Vec<TestSuite> {
    let mut suites_map: std::collections::HashMap<String, Vec<TestCase>> =
        std::collections::HashMap::new();

    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // "[ RUN      ] MathTest.TestAdd"
        if trimmed.starts_with("[ RUN") {
            let test_full = trimmed
                .strip_prefix("[ RUN")
                .unwrap_or("")
                .trim()
                .trim_start_matches(']')
                .trim();

            let (suite_name, test_name) = split_gtest_name(test_full);

            // Collect lines until we find OK/FAILED
            let mut output_lines = Vec::new();
            i += 1;

            let mut status = TestStatus::Passed;
            let mut duration = Duration::from_millis(0);

            while i < lines.len() {
                let line = lines[i].trim();

                if line.starts_with("[       OK ]") || line.starts_with("[  FAILED  ]") {
                    status = if line.starts_with("[       OK ]") {
                        TestStatus::Passed
                    } else {
                        TestStatus::Failed
                    };
                    duration = parse_gtest_duration(line);
                    break;
                }

                if !line.is_empty() && !line.starts_with("[") {
                    output_lines.push(line.to_string());
                }

                i += 1;
            }

            let error = if status == TestStatus::Failed && !output_lines.is_empty() {
                let message = output_lines
                    .iter()
                    .find(|l| is_cpp_error_line(l))
                    .cloned()
                    .unwrap_or_else(|| output_lines[0].clone());
                let location = output_lines
                    .iter()
                    .find_map(|l| extract_file_line_location(l));
                Some(TestError { message, location })
            } else {
                None
            };

            suites_map.entry(suite_name).or_default().push(TestCase {
                name: test_name,
                status,
                duration,
                error,
            });
        }

        i += 1;
    }

    let mut suites: Vec<TestSuite> = suites_map
        .into_iter()
        .map(|(name, tests)| TestSuite { name, tests })
        .collect();
    suites.sort_by(|a, b| a.name.cmp(&b.name));

    suites
}

/// Split a Google Test full name "SuiteName.TestName" into parts.
fn split_gtest_name(full_name: &str) -> (String, String) {
    if let Some(dot) = full_name.find('.') {
        (
            full_name[..dot].to_string(),
            full_name[dot + 1..].to_string(),
        )
    } else {
        ("tests".into(), full_name.to_string())
    }
}

/// Parse duration from a GTest OK/FAILED line: "[       OK ] Test (123 ms)"
fn parse_gtest_duration(line: &str) -> Duration {
    if let Some(paren_start) = line.rfind('(') {
        let inside = &line[paren_start + 1..line.len().saturating_sub(1)];
        let inside = inside.trim();
        if inside.ends_with("ms") {
            let num_str = inside.strip_suffix("ms").unwrap_or("").trim();
            if let Ok(ms) = num_str.parse::<u64>() {
                return Duration::from_millis(ms);
            }
        }
    }
    Duration::from_millis(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_cmake_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.14)\nenable_testing()\n",
        )
        .unwrap();
        let adapter = CppAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "C/C++");
        assert_eq!(det.framework, "ctest");
        assert!((det.confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn detect_meson_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("meson.build"), "project('test', 'c')\n").unwrap();
        let adapter = CppAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "meson test");
    }

    #[test]
    fn detect_no_cpp() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = CppAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_ctest_detailed_output() {
        let stdout = r#"
Test project /home/user/project/build
    Start 1: test_basic
1/3 Test #1: test_basic ...................   Passed    0.01 sec
    Start 2: test_advanced
2/3 Test #2: test_advanced ................   Passed    0.02 sec
    Start 3: test_edge
3/3 Test #3: test_edge ....................***Failed    0.01 sec

67% tests passed, 1 tests failed out of 3

Total Test time (real) =   0.04 sec
"#;
        let adapter = CppAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_ctest_all_pass() {
        let stdout = r#"
Test project /home/user/project/build
    Start 1: test_one
1/2 Test #1: test_one .....................   Passed    0.01 sec
    Start 2: test_two
2/2 Test #2: test_two .....................   Passed    0.01 sec

100% tests passed, 0 tests failed out of 2

Total Test time (real) =   0.02 sec
"#;
        let adapter = CppAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_ctest_summary_only() {
        let stdout = "67% tests passed, 1 tests failed out of 3\n";
        let adapter = CppAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_ctest_empty_output() {
        let adapter = CppAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_ctest_duration_value() {
        assert_eq!(
            parse_ctest_duration("Total Test time (real) =   0.05 sec"),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn find_build_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("build")).unwrap();
        assert!(CppAdapter::find_build_dir(dir.path()).is_some());
    }

    #[test]
    fn find_build_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(CppAdapter::find_build_dir(dir.path()).is_none());
    }

    #[test]
    fn parse_gtest_detailed_output() {
        let stdout = r#"
[==========] Running 3 tests from 1 test suite.
[----------] 3 tests from MathTest
[ RUN      ] MathTest.TestAdd
[       OK ] MathTest.TestAdd (0 ms)
[ RUN      ] MathTest.TestSub
[       OK ] MathTest.TestSub (1 ms)
[ RUN      ] MathTest.TestDiv
test_math.cpp:42: Failure
Expected equality of these values:
  divide(10, 3)
    Which is: 3
  4
[  FAILED  ] MathTest.TestDiv (0 ms)
[----------] 3 tests from MathTest (1 ms total)
[==========] 3 tests from 1 test suite ran. (1 ms total)
[  PASSED  ] 2 tests.
[  FAILED  ] 1 test, listed below:
[  FAILED  ] MathTest.TestDiv
"#;
        let suites = parse_gtest_output(stdout);
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].name, "MathTest");
        assert_eq!(suites[0].tests.len(), 3);
        assert_eq!(suites[0].tests[0].name, "TestAdd");
        assert_eq!(suites[0].tests[0].status, TestStatus::Passed);
        assert_eq!(suites[0].tests[2].name, "TestDiv");
        assert_eq!(suites[0].tests[2].status, TestStatus::Failed);
        assert!(suites[0].tests[2].error.is_some());
    }

    #[test]
    fn parse_gtest_all_pass() {
        let stdout = r#"
[==========] Running 2 tests from 1 test suite.
[ RUN      ] MathTest.TestAdd
[       OK ] MathTest.TestAdd (0 ms)
[ RUN      ] MathTest.TestSub
[       OK ] MathTest.TestSub (0 ms)
[==========] 2 tests from 1 test suite ran. (0 ms total)
[  PASSED  ] 2 tests.
"#;
        let suites = parse_gtest_output(stdout);
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].tests.len(), 2);
        assert!(
            suites[0]
                .tests
                .iter()
                .all(|t| t.status == TestStatus::Passed)
        );
    }

    #[test]
    fn parse_gtest_multiple_suites() {
        let stdout = r#"
[ RUN      ] MathTest.TestAdd
[       OK ] MathTest.TestAdd (0 ms)
[ RUN      ] StringTest.TestUpper
[       OK ] StringTest.TestUpper (0 ms)
"#;
        let suites = parse_gtest_output(stdout);
        assert_eq!(suites.len(), 2);
    }

    #[test]
    fn parse_gtest_failure_with_error_details() {
        let stdout = r#"
[ RUN      ] MathTest.TestDiv
test_math.cpp:42: Failure
Expected: 4
  Actual: 3
[  FAILED  ] MathTest.TestDiv (0 ms)
"#;
        let suites = parse_gtest_output(stdout);
        let err = suites[0].tests[0].error.as_ref().unwrap();
        assert!(err.location.is_some());
        assert!(err.location.as_ref().unwrap().contains("test_math.cpp:42"));
    }

    #[test]
    fn parse_gtest_duration_test() {
        assert_eq!(
            parse_gtest_duration("[       OK ] MathTest.TestAdd (123 ms)"),
            Duration::from_millis(123)
        );
        assert_eq!(
            parse_gtest_duration("[  FAILED  ] MathTest.TestDiv (0 ms)"),
            Duration::from_millis(0)
        );
    }

    #[test]
    fn split_gtest_name_test() {
        assert_eq!(
            split_gtest_name("MathTest.TestAdd"),
            ("MathTest".into(), "TestAdd".into())
        );
        assert_eq!(
            split_gtest_name("SimpleTest"),
            ("tests".into(), "SimpleTest".into())
        );
    }

    #[test]
    fn is_cpp_error_line_test() {
        assert!(is_cpp_error_line("ASSERT_EQ failed"));
        assert!(is_cpp_error_line("error: expected 4"));
        assert!(is_cpp_error_line("Failure"));
        assert!(is_cpp_error_line("Segfault at 0x0"));
        assert!(!is_cpp_error_line("Running tests..."));
    }

    #[test]
    fn extract_file_line_location_test() {
        assert_eq!(
            extract_file_line_location("test.cpp:42: Failure"),
            Some("test.cpp:42".into())
        );
        assert_eq!(
            extract_file_line_location("main.c:10: error: boom"),
            Some("main.c:10".into())
        );
        assert_eq!(
            extract_file_line_location("test.hpp(15): fatal"),
            Some("test.hpp(15)".into())
        );
        assert!(extract_file_line_location("no file here").is_none());
    }

    #[test]
    fn extract_ctest_name_test() {
        assert_eq!(
            extract_ctest_name("1/3 Test #1: test_basic ...................   Passed    0.01 sec"),
            "test_basic"
        );
    }

    #[test]
    fn parse_ctest_failures_with_output() {
        let output = r#"
1/2 Test #1: test_pass ...................   Passed    0.01 sec
2/2 Test #2: test_edge ....................***Failed    0.01 sec
ASSERT_EQ(4, result) failed
  Expected: 4
  Actual: 3
test_edge.cpp:42: Failure

67% tests passed, 1 tests failed out of 2
"#;
        let failures = parse_ctest_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].test_name, "test_edge");
        assert!(failures[0].error_line.is_some());
    }

    #[test]
    fn truncate_output_test() {
        assert_eq!(truncate_output("short", 100), "short");
        let long = "x".repeat(1000);
        let truncated = truncate_output(&long, 800);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn first_meaningful_line_test() {
        assert_eq!(
            first_meaningful_line("---\n\nHello world\nmore"),
            "Hello world"
        );
        assert_eq!(first_meaningful_line("first"), "first");
    }

    #[test]
    fn parse_ctest_with_gtest_output() {
        let stdout = r#"
Test project /home/user/project/build
    Start 1: gtest_math
1/1 Test #1: gtest_math .....................***Failed    0.01 sec
[==========] Running 2 tests from 1 test suite.
[ RUN      ] MathTest.TestAdd
[       OK ] MathTest.TestAdd (0 ms)
[ RUN      ] MathTest.TestDiv
test.cpp:10: Failure
Expected: 4
  Actual: 3
[  FAILED  ] MathTest.TestDiv (0 ms)
[  PASSED  ] 1 test.
[  FAILED  ] 1 test, listed below:
[  FAILED  ] MathTest.TestDiv

67% tests passed, 1 tests failed out of 1

Total Test time (real) =   0.01 sec
"#;
        let adapter = CppAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        // GTest output should be preferred
        assert_eq!(result.suites.len(), 1);
        assert_eq!(result.suites[0].name, "MathTest");
        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }
}
