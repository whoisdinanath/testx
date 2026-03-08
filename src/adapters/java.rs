use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::{DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite};

pub struct JavaAdapter;

impl Default for JavaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Detect build tool: Maven or Gradle
    fn detect_build_tool(project_dir: &Path) -> Option<&'static str> {
        // Gradle takes priority (more modern)
        if project_dir.join("build.gradle.kts").exists()
            || project_dir.join("build.gradle").exists()
        {
            return Some("gradle");
        }
        if project_dir.join("pom.xml").exists() {
            return Some("maven");
        }
        None
    }

    /// Check for Gradle wrapper
    fn has_gradle_wrapper(project_dir: &Path) -> bool {
        project_dir.join("gradlew").exists()
    }
}

impl TestAdapter for JavaAdapter {
    fn name(&self) -> &str {
        "Java/Kotlin"
    }

    fn check_runner(&self) -> Option<String> {
        // Will check in build_command based on build tool
        None
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let build_tool = Self::detect_build_tool(project_dir)?;

        let framework = match build_tool {
            "gradle" => {
                if project_dir.join("build.gradle.kts").exists() {
                    "gradle (kotlin dsl)"
                } else {
                    "gradle"
                }
            }
            "maven" => "maven surefire",
            _ => "unknown",
        };

        Some(DetectionResult {
            language: "Java".into(),
            framework: framework.into(),
            confidence: 0.95,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let build_tool = Self::detect_build_tool(project_dir).unwrap_or("maven");

        let mut cmd;

        match build_tool {
            "gradle" => {
                if Self::has_gradle_wrapper(project_dir) {
                    cmd = Command::new("./gradlew");
                } else {
                    cmd = Command::new("gradle");
                }
                cmd.arg("test");
            }
            _ => {
                // Maven
                cmd = Command::new("mvn");
                cmd.arg("test");
                cmd.arg("-B"); // batch mode (no interactive)
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

        // Try Maven Surefire parsing first, then Gradle
        let suites = if combined.contains("Tests run:") {
            parse_maven_output(&combined, exit_code)
        } else {
            parse_gradle_output(&combined, exit_code)
        };

        let duration = parse_java_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse Maven Surefire output.
///
/// Format:
/// ```text
/// [INFO] -------------------------------------------------------
/// [INFO]  T E S T S
/// [INFO] -------------------------------------------------------
/// [INFO] Running com.example.AppTest
/// [INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 0.05 s
/// [INFO]
/// [INFO] Results:
/// [INFO]
/// [INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0
/// ```
fn parse_maven_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut suites = Vec::new();
    let mut current_suite = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        // Strip [INFO] / [ERROR] prefix
        let clean = trimmed
            .strip_prefix("[INFO] ")
            .or_else(|| trimmed.strip_prefix("[ERROR] "))
            .unwrap_or(trimmed)
            .trim();

        // Suite start: "Running com.example.AppTest"
        if let Some(rest) = clean.strip_prefix("Running ") {
            current_suite = rest.trim().to_string();
            continue;
        }

        // Result line: "Tests run: 3, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 0.05 s"
        if clean.starts_with("Tests run:")
            && !current_suite.is_empty()
            && let Some(suite) = parse_surefire_result_line(clean, &current_suite)
        {
            suites.push(suite);
        }
        // Don't reset current_suite — Maven repeats the summary at the end
    }

    // Deduplicate: Maven prints per-class results AND a final summary.
    // Keep only the per-class results (which have specific suite names).
    // If we only got the summary, keep that.
    if suites.len() > 1 {
        // Remove any suite that's just a repeat of the totals (has same name as another)
        let mut seen = std::collections::HashSet::new();
        suites.retain(|s| seen.insert(s.name.clone()));
    }

    if suites.is_empty() {
        let status = if exit_code == 0 {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };
        suites.push(TestSuite {
            name: "tests".into(),
            tests: vec![TestCase {
                name: "test_suite".into(),
                status,
                duration: Duration::from_millis(0),
                error: None,
            }],
        });
    }

    suites
}

fn parse_surefire_result_line(line: &str, suite_name: &str) -> Option<TestSuite> {
    // "Tests run: 3, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 0.05 s"
    let mut total = 0usize;
    let mut failures = 0usize;
    let mut errors = 0usize;
    let mut skipped = 0usize;

    for part in line.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("Tests run:") {
            total = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = part.strip_prefix("Failures:") {
            failures = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = part.strip_prefix("Errors:") {
            errors = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = part.strip_prefix("Skipped:") {
            skipped = rest.trim().parse().unwrap_or(0);
        }
    }

    if total == 0 && failures == 0 {
        return None;
    }

    let failed = failures + errors;
    let passed = total.saturating_sub(failed + skipped);

    let mut tests = Vec::new();
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

    Some(TestSuite {
        name: suite_name.to_string(),
        tests,
    })
}

/// Parse Gradle test output.
///
/// Format:
/// ```text
/// > Task :test
///
/// com.example.AppTest > testAdd PASSED
/// com.example.AppTest > testDivide FAILED
///
/// 3 tests completed, 1 failed
/// ```
fn parse_gradle_output(output: &str, exit_code: i32) -> Vec<TestSuite> {
    let mut suites_map: std::collections::HashMap<String, Vec<TestCase>> =
        std::collections::HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // "com.example.AppTest > testAdd PASSED"
        if trimmed.contains(" > ")
            && (trimmed.ends_with("PASSED")
                || trimmed.ends_with("FAILED")
                || trimmed.ends_with("SKIPPED"))
        {
            let status = if trimmed.ends_with("PASSED") {
                TestStatus::Passed
            } else if trimmed.ends_with("FAILED") {
                TestStatus::Failed
            } else {
                TestStatus::Skipped
            };

            if let Some(arrow_idx) = trimmed.find(" > ") {
                let suite_name = trimmed[..arrow_idx].trim().to_string();
                let rest = &trimmed[arrow_idx + 3..];
                // Strip status suffix
                let test_name = rest
                    .rsplit_once(' ')
                    .map(|(name, _)| name.trim())
                    .unwrap_or(rest)
                    .to_string();

                suites_map.entry(suite_name).or_default().push(TestCase {
                    name: test_name,
                    status,
                    duration: Duration::from_millis(0),
                    error: None,
                });
            }
        }
    }

    let mut suites: Vec<TestSuite> = suites_map
        .into_iter()
        .map(|(name, tests)| TestSuite { name, tests })
        .collect();
    suites.sort_by(|a, b| a.name.cmp(&b.name));

    // Fallback: parse summary line "X tests completed, Y failed"
    if suites.is_empty() {
        suites.push(parse_gradle_summary(output, exit_code));
    }

    suites
}

fn parse_gradle_summary(output: &str, exit_code: i32) -> TestSuite {
    let mut passed = 0usize;
    let mut failed = 0usize;

    for line in output.lines() {
        let trimmed = line.trim();
        // "3 tests completed, 1 failed"
        if trimmed.contains("tests completed") {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("completed")
                    && let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok())
                {
                    passed = n;
                }
                if part.contains("failed")
                    && let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok())
                {
                    failed = n;
                    passed = passed.saturating_sub(failed);
                }
            }
        }
    }

    let mut tests = Vec::new();
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

    TestSuite {
        name: "tests".into(),
        tests,
    }
}

fn parse_java_duration(output: &str) -> Option<Duration> {
    // Maven: "Time elapsed: 0.05 s" or "Total time:  1.234 s"
    for line in output.lines() {
        if let Some(idx) = line.find("Time elapsed:") {
            let after = &line[idx + 13..];
            let num_str: String = after
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(secs) = num_str.parse::<f64>() {
                return Some(Duration::from_secs_f64(secs));
            }
        }
        // Gradle: "BUILD SUCCESSFUL in 2s" or "BUILD FAILED in 1s"
        if (line.contains("BUILD SUCCESSFUL") || line.contains("BUILD FAILED"))
            && line.contains(" in ")
            && let Some(idx) = line.rfind(" in ")
        {
            let after = &line[idx + 4..];
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
    fn detect_maven_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pom.xml"),
            "<project><modelVersion>4.0.0</modelVersion></project>",
        )
        .unwrap();
        let adapter = JavaAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Java");
        assert_eq!(det.framework, "maven surefire");
    }

    #[test]
    fn detect_gradle_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle"), "apply plugin: 'java'\n").unwrap();
        let adapter = JavaAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.language, "Java");
        assert_eq!(det.framework, "gradle");
    }

    #[test]
    fn detect_gradle_kts_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle.kts"), "plugins { java }\n").unwrap();
        let adapter = JavaAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "gradle (kotlin dsl)");
    }

    #[test]
    fn detect_no_java() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = JavaAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_maven_surefire_output() {
        let stdout = r#"
[INFO] -------------------------------------------------------
[INFO]  T E S T S
[INFO] -------------------------------------------------------
[INFO] Running com.example.AppTest
[INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 0.05 s
[INFO]
[INFO] Results:
[INFO]
[INFO] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0
[INFO]
[INFO] BUILD FAILURE
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_maven_all_pass() {
        let stdout = r#"
[INFO] Running com.example.MathTest
[INFO] Tests run: 5, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 0.12 s
[INFO] BUILD SUCCESS
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 5);
        assert_eq!(result.total_passed(), 5);
        assert!(result.is_success());
    }

    #[test]
    fn parse_maven_with_skipped() {
        let stdout = r#"
[INFO] Running com.example.AppTest
[INFO] Tests run: 4, Failures: 0, Errors: 0, Skipped: 2, Time elapsed: 0.03 s
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_skipped(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_maven_with_errors() {
        let stdout = r#"
[INFO] Running com.example.AppTest
[INFO] Tests run: 3, Failures: 0, Errors: 2, Skipped: 0, Time elapsed: 0.01 s
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_failed(), 2);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_gradle_test_output() {
        let stdout = r#"
> Task :test

com.example.AppTest > testAdd PASSED
com.example.AppTest > testSubtract PASSED
com.example.AppTest > testDivide FAILED

3 tests completed, 1 failed

BUILD FAILED in 2s
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn parse_gradle_all_pass() {
        let stdout = r#"
> Task :test

com.example.MathTest > testAdd PASSED
com.example.MathTest > testMultiply PASSED

BUILD SUCCESSFUL in 3s
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_tests(), 2);
        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn parse_gradle_multiple_suites() {
        let stdout = r#"
com.example.MathTest > testAdd PASSED
com.example.StringTest > testUpper PASSED
com.example.StringTest > testLower FAILED

3 tests completed, 1 failed
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.suites.len(), 2);
        assert_eq!(result.total_tests(), 3);
    }

    #[test]
    fn parse_java_empty_output() {
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_java_duration_maven() {
        assert_eq!(
            parse_java_duration(
                "[INFO] Tests run: 3, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 1.23 s"
            ),
            Some(Duration::from_millis(1230))
        );
    }

    #[test]
    fn parse_java_duration_gradle() {
        assert_eq!(
            parse_java_duration("BUILD SUCCESSFUL in 5s"),
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn gradle_wrapper_detection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle"), "").unwrap();
        assert!(!JavaAdapter::has_gradle_wrapper(dir.path()));
        std::fs::write(dir.path().join("gradlew"), "#!/bin/bash\n").unwrap();
        assert!(JavaAdapter::has_gradle_wrapper(dir.path()));
    }
}
