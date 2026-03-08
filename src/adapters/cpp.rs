use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

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

        let suites = parse_ctest_output(&combined, exit_code);
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
            .map(Duration::from_secs_f64)
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
}
