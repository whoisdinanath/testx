use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestError, TestRunResult, TestStatus,
    TestSuite,
};

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
        // check_runner() has no access to project_dir, so we can't check for
        // ./gradlew. We check for system-installed tools; build_command will
        // fall back to ./gradlew if it exists. Only report missing if neither
        // system gradle nor mvn nor ant are available.
        if which::which("gradle").is_ok()
            || which::which("mvn").is_ok()
            || which::which("ant").is_ok()
        {
            return None;
        }
        // Note: this may false-positive if the project has a ./gradlew wrapper.
        // build_command handles that case. We still warn so users know early.
        Some("gradle, mvn, or ant (or add a ./gradlew wrapper)".into())
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

        let has_wrapper =
            Self::has_gradle_wrapper(project_dir) || project_dir.join(".mvn").is_dir();
        let has_test_dir = project_dir.join("src/test").is_dir();
        let has_runner = which::which("gradle").is_ok()
            || which::which("mvn").is_ok()
            || Self::has_gradle_wrapper(project_dir);

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, has_wrapper)
            .signal(0.15, has_test_dir)
            .signal(0.10, has_runner)
            .finish();

        Some(DetectionResult {
            language: "Java".into(),
            framework: framework.into(),
            confidence,
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
        let mut suites = if combined.contains("Tests run:") {
            parse_maven_output(&combined, exit_code)
        } else {
            parse_gradle_output(&combined, exit_code)
        };

        // Enrich with failure details from Maven or Gradle output
        let failures = parse_java_failures(&combined);
        if !failures.is_empty() {
            enrich_with_errors(&mut suites, &failures);
        }

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
                return Some(duration_from_secs_safe(secs));
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
                return Some(duration_from_secs_safe(secs));
            }
        }
    }
    None
}

/// A parsed test failure from Maven/Gradle output.
#[derive(Debug, Clone)]
struct JavaTestFailure {
    /// The fully-qualified test name (e.g., "testAdd(com.example.MathTest)")
    test_name: String,
    /// Short test method name
    method_name: String,
    /// The error message
    message: String,
    /// Stack trace snippet (first few lines)
    stack_trace: Option<String>,
}

/// Parse Java test failures from Maven Surefire or Gradle output.
///
/// Maven format:
/// ```text
/// [ERROR] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0
/// [ERROR] Failures:
/// [ERROR]   AppTest.testDivide:42 expected:<4> but was:<3>
/// ```
///
/// Or verbose Maven format:
/// ```text
/// Failed tests:
///   testDivide(com.example.AppTest): expected:<4> but was:<3>
///
/// Tests in error:
///   testBroken(com.example.AppTest): NullPointerException
/// ```
///
/// Gradle format:
/// ```text
/// com.example.AppTest > testDivide FAILED
///     org.opentest4j.AssertionFailedError: expected: <4> but was: <3>
///         at com.example.AppTest.testDivide(AppTest.java:42)
/// ```
fn parse_java_failures(output: &str) -> Vec<JavaTestFailure> {
    let failures = Vec::new();

    // Try Maven "Failed tests:" section
    let maven_failures = parse_maven_failed_tests_section(output);
    if !maven_failures.is_empty() {
        return maven_failures;
    }

    // Try Gradle inline failure output
    let gradle_failures = parse_gradle_failures(output);
    if !gradle_failures.is_empty() {
        return gradle_failures;
    }

    // Try Maven [ERROR] Failures: section
    let error_failures = parse_maven_error_failures(output);
    if !error_failures.is_empty() {
        return error_failures;
    }

    failures
}

/// Parse Maven "Failed tests:" and "Tests in error:" sections.
fn parse_maven_failed_tests_section(output: &str) -> Vec<JavaTestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;
    let mut in_section = false;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let clean = strip_maven_prefix(trimmed);

        if clean == "Failed tests:" || clean == "Tests in error:" {
            in_section = true;
            i += 1;
            continue;
        }

        if in_section {
            // End of section: empty line or new section header
            if clean.is_empty()
                || clean.starts_with("Tests run:")
                || clean == "Failed tests:"
                || clean == "Tests in error:"
            {
                if clean == "Failed tests:" || clean == "Tests in error:" {
                    continue;
                }
                in_section = false;
                i += 1;
                continue;
            }

            // "  testDivide(com.example.AppTest): expected:<4> but was:<3>"
            if let Some(failure) = parse_maven_failure_line(clean) {
                failures.push(failure);
            }
        }

        i += 1;
    }

    failures
}

/// Parse a single Maven failure line:
/// "testDivide(com.example.AppTest): expected:<4> but was:<3>"
fn parse_maven_failure_line(line: &str) -> Option<JavaTestFailure> {
    // Format: "methodName(ClassName): message"
    let paren_open = line.find('(')?;
    let paren_close = line.find(')')?;
    if paren_close <= paren_open {
        return None;
    }

    let method_name = line[..paren_open].trim().to_string();
    let _class_name = &line[paren_open + 1..paren_close];
    let test_name = line[..paren_close + 1].trim().to_string();

    let message = if paren_close + 1 < line.len() {
        let rest = &line[paren_close + 1..];
        rest.strip_prefix(':')
            .or_else(|| rest.strip_prefix(": "))
            .unwrap_or(rest)
            .trim()
            .to_string()
    } else {
        String::new()
    };

    Some(JavaTestFailure {
        test_name,
        method_name,
        message,
        stack_trace: None,
    })
}

/// Parse Gradle inline failure blocks.
/// ```text
/// com.example.AppTest > testDivide FAILED
///     org.opentest4j.AssertionFailedError: expected: <4> but was: <3>
///         at com.example.AppTest.testDivide(AppTest.java:42)
/// ```
fn parse_gradle_failures(output: &str) -> Vec<JavaTestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // "com.example.AppTest > testDivide FAILED"
        if trimmed.contains(" > ") && trimmed.ends_with("FAILED") {
            let arrow_idx = trimmed.find(" > ").unwrap();
            let class_name = &trimmed[..arrow_idx];
            let rest = &trimmed[arrow_idx + 3..];
            let method_name = rest
                .strip_suffix(" FAILED")
                .unwrap_or(rest)
                .trim()
                .to_string();

            let test_name = format!("{}.{}", class_name, method_name);

            // Collect following indented lines as error details
            let mut message_lines = Vec::new();
            let mut stack_lines = Vec::new();
            i += 1;

            while i < lines.len() {
                let line = lines[i];
                if !line.starts_with("    ") && !line.starts_with('\t') {
                    break;
                }
                let content = line.trim();
                if content.starts_with("at ") {
                    stack_lines.push(content.to_string());
                } else if !content.is_empty() {
                    message_lines.push(content.to_string());
                }
                i += 1;
            }

            let message = message_lines.join("\n");
            let stack_trace = if stack_lines.is_empty() {
                None
            } else {
                Some(
                    stack_lines
                        .into_iter()
                        .take(5)
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            };

            failures.push(JavaTestFailure {
                test_name,
                method_name,
                message: truncate_java_message(&message, 500),
                stack_trace,
            });
            continue;
        }

        i += 1;
    }

    failures
}

/// Parse Maven [ERROR] section failures.
/// "[ERROR]   AppTest.testDivide:42 expected:<4> but was:<3>"
fn parse_maven_error_failures(output: &str) -> Vec<JavaTestFailure> {
    let mut failures = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut in_failures = false;

    for line in &lines {
        let trimmed = line.trim();
        let clean = strip_maven_prefix(trimmed);

        if clean == "Failures:" || clean == "Errors:" {
            in_failures = true;
            continue;
        }

        if in_failures && !clean.is_empty() {
            // "  AppTest.testDivide:42 expected:<4> but was:<3>"
            if clean.contains('.') && (clean.contains(':') || clean.contains(' ')) {
                let parts: Vec<&str> = clean.splitn(2, ' ').collect();
                if !parts.is_empty() {
                    let test_ref = parts[0];
                    let message = if parts.len() > 1 {
                        parts[1].to_string()
                    } else {
                        String::new()
                    };

                    // Extract method name from "AppTest.testDivide:42"
                    let method_name = test_ref
                        .split('.')
                        .next_back()
                        .unwrap_or(test_ref)
                        .split(':')
                        .next()
                        .unwrap_or(test_ref)
                        .to_string();

                    failures.push(JavaTestFailure {
                        test_name: test_ref.to_string(),
                        method_name,
                        message: truncate_java_message(&message, 500),
                        stack_trace: None,
                    });
                }
            } else if clean.starts_with("Tests run:") || clean.starts_with("[") {
                in_failures = false;
            }
        }
    }

    failures
}

/// Strip Maven log prefix: "[INFO] " or "[ERROR] "
fn strip_maven_prefix(line: &str) -> &str {
    line.strip_prefix("[INFO] ")
        .or_else(|| line.strip_prefix("[ERROR] "))
        .or_else(|| line.strip_prefix("[WARNING] "))
        .unwrap_or(line)
        .trim()
}

/// Enrich test cases with failure details.
fn enrich_with_errors(suites: &mut [TestSuite], failures: &[JavaTestFailure]) {
    for suite in suites.iter_mut() {
        for test in suite.tests.iter_mut() {
            if test.status != TestStatus::Failed || test.error.is_some() {
                continue;
            }
            if let Some(failure) = find_matching_java_failure(&test.name, &suite.name, failures) {
                let location = failure
                    .stack_trace
                    .as_ref()
                    .and_then(|st| st.lines().next())
                    .map(|s| s.to_string());
                test.error = Some(TestError {
                    message: failure.message.clone(),
                    location,
                });
            }
        }
    }
}

/// Find a matching failure for a test in the failures list.
fn find_matching_java_failure<'a>(
    test_name: &str,
    suite_name: &str,
    failures: &'a [JavaTestFailure],
) -> Option<&'a JavaTestFailure> {
    for failure in failures {
        // Direct method name match
        if test_name == failure.method_name {
            return Some(failure);
        }
        // Full test name match (Gradle style: "com.example.AppTest.testMethod")
        if failure.test_name.ends_with(&format!(".{}", test_name)) {
            return Some(failure);
        }
        // Check if test_name is contained in the failure's full name
        if failure.test_name.contains(test_name) {
            return Some(failure);
        }
        // Suite + method match
        if failure.test_name.contains(suite_name) && failure.method_name == test_name {
            return Some(failure);
        }
    }
    // Single failure for synthetic name
    if failures.len() == 1 && test_name.starts_with("failed_test_") {
        return Some(&failures[0]);
    }
    None
}

/// Truncate a message to max length.
fn truncate_java_message(msg: &str, max_len: usize) -> String {
    if msg.len() <= max_len {
        msg.to_string()
    } else {
        format!("{}...", &msg[..max_len])
    }
}

/// Parse Surefire XML test report files from standard locations.
/// Returns parsed test suites from XML report files found at:
/// - target/surefire-reports/TEST-*.xml (Maven)
/// - build/test-results/test/TEST-*.xml (Gradle)
pub fn parse_surefire_xml(project_dir: &Path) -> Vec<TestSuite> {
    let report_dirs = [
        project_dir.join("target/surefire-reports"),
        project_dir.join("build/test-results/test"),
        project_dir.join("build/test-results"),
    ];

    let mut suites = Vec::new();

    for dir in &report_dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("TEST-")
                    && name.ends_with(".xml")
                    && let Ok(content) = std::fs::read_to_string(entry.path())
                    && let Some(suite) = parse_single_surefire_xml(&content)
                {
                    suites.push(suite);
                }
            }
        }
    }

    suites
}

/// Parse a single Surefire/JUnit XML report file.
///
/// Format:
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <testsuite name="com.example.AppTest" tests="3" failures="1" errors="0" skipped="0" time="0.05">
///   <testcase name="testAdd" classname="com.example.AppTest" time="0.01"/>
///   <testcase name="testSub" classname="com.example.AppTest" time="0.02"/>
///   <testcase name="testDiv" classname="com.example.AppTest" time="0.02">
///     <failure message="expected:&lt;4&gt; but was:&lt;3&gt;" type="AssertionError">
///       stack trace here
///     </failure>
///   </testcase>
/// </testsuite>
/// ```
fn parse_single_surefire_xml(content: &str) -> Option<TestSuite> {
    // Extract suite name
    let suite_name = extract_xml_attr(content, "testsuite", "name")?;

    let mut tests = Vec::new();

    // Find all <testcase> elements
    let mut search_from = 0;
    while let Some(tc_start) = content[search_from..].find("<testcase") {
        let absolute_start = search_from + tc_start;

        // Find the end of this testcase element
        let tc_content_start = absolute_start + 9; // skip "<testcase"
        let (tc_end, is_self_closing) =
            if let Some(self_close) = find_self_closing_end(content, tc_content_start) {
                (self_close, true)
            } else if let Some(close) = content[tc_content_start..].find("</testcase>") {
                (tc_content_start + close + 11, false)
            } else {
                break;
            };

        let tc_text = &content[absolute_start..tc_end];

        let name =
            extract_xml_attr(tc_text, "testcase", "name").unwrap_or_else(|| "unknown".into());
        let time_str = extract_xml_attr(tc_text, "testcase", "time").unwrap_or_default();
        let duration = time_str
            .parse::<f64>()
            .map(duration_from_secs_safe)
            .unwrap_or(Duration::from_millis(0));

        let (status, error) = if is_self_closing {
            // Self-closing means passed (no failure/error/skipped child)
            (TestStatus::Passed, None)
        } else if tc_text.contains("<failure") {
            let msg = extract_xml_attr(tc_text, "failure", "message")
                .unwrap_or_else(|| "Test failed".into());
            let error_type = extract_xml_attr(tc_text, "failure", "type");
            let location = error_type.map(|t| format!("type: {}", t));
            (
                TestStatus::Failed,
                Some(TestError {
                    message: xml_unescape(&msg),
                    location,
                }),
            )
        } else if tc_text.contains("<error") {
            let msg = extract_xml_attr(tc_text, "error", "message")
                .unwrap_or_else(|| "Test error".into());
            (
                TestStatus::Failed,
                Some(TestError {
                    message: xml_unescape(&msg),
                    location: None,
                }),
            )
        } else if tc_text.contains("<skipped") {
            (TestStatus::Skipped, None)
        } else {
            (TestStatus::Passed, None)
        };

        tests.push(TestCase {
            name,
            status,
            duration,
            error,
        });

        search_from = tc_end;
    }

    if tests.is_empty() {
        return None;
    }

    Some(TestSuite {
        name: suite_name,
        tests,
    })
}

/// Find the end of a self-closing XML tag (/>).
/// Returns the position after the /> if the tag closes itself (no children).
/// Only checks the opening tag itself — not child elements.
fn find_self_closing_end(content: &str, from: usize) -> Option<usize> {
    let remaining = &content[from..];
    // Find the first '>' character — this ends the opening tag
    let first_close = remaining.find('>')?;
    // Check if it's "/>" (self-closing)
    if first_close > 0 && remaining.as_bytes()[first_close - 1] == b'/' {
        Some(from + first_close + 1)
    } else {
        None
    }
}

/// Extract an XML attribute value from a tag.
/// Simple string-based extraction - not a full XML parser.
fn extract_xml_attr(content: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = content.find(&format!("<{}", tag))?;
    let tag_content = &content[tag_start..];
    let tag_end = tag_content.find('>')?.min(tag_content.len());
    let tag_text = &tag_content[..tag_end];

    let attr_pattern = format!("{}=\"", attr);
    let attr_start = tag_text.find(&attr_pattern)?;
    let value_start = attr_start + attr_pattern.len();
    let value_end = tag_text[value_start..].find('"')?;
    Some(tag_text[value_start..value_start + value_end].to_string())
}

/// Unescape basic XML entities.
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
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

    #[test]
    fn parse_maven_failed_tests_section_test() {
        let output = r#"
[ERROR] Failed tests:
[ERROR]   testDivide(com.example.MathTest): expected:<4> but was:<3>
[ERROR]   testModulo(com.example.MathTest): ArithmeticException
[ERROR]
[ERROR] Tests run: 5, Failures: 2, Errors: 0, Skipped: 0
"#;
        let failures = parse_maven_failed_tests_section(output);
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].method_name, "testDivide");
        assert!(failures[0].message.contains("expected:<4>"));
        assert_eq!(failures[1].method_name, "testModulo");
    }

    #[test]
    fn parse_maven_failure_line_test() {
        let failure =
            parse_maven_failure_line("testAdd(com.example.MathTest): expected 5 got 4").unwrap();
        assert_eq!(failure.method_name, "testAdd");
        assert_eq!(failure.test_name, "testAdd(com.example.MathTest)");
        assert_eq!(failure.message, "expected 5 got 4");
    }

    #[test]
    fn parse_maven_failure_line_no_message() {
        let failure = parse_maven_failure_line("testAdd(com.example.MathTest)").unwrap();
        assert_eq!(failure.method_name, "testAdd");
        assert!(failure.message.is_empty());
    }

    #[test]
    fn parse_gradle_failure_blocks() {
        let output = r#"
> Task :test

com.example.AppTest > testAdd PASSED
com.example.AppTest > testDivide FAILED
    org.opentest4j.AssertionFailedError: expected: <4> but was: <3>
        at com.example.AppTest.testDivide(AppTest.java:42)

3 tests completed, 1 failed
"#;
        let failures = parse_gradle_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].method_name, "testDivide");
        assert!(failures[0].message.contains("AssertionFailedError"));
        assert!(failures[0].stack_trace.is_some());
    }

    #[test]
    fn parse_gradle_multiple_failures() {
        let output = r#"
com.example.Test > methodA FAILED
    java.lang.RuntimeException: boom
com.example.Test > methodB FAILED
    java.lang.NullPointerException
        at com.example.Test.methodB(Test.java:10)
"#;
        let failures = parse_gradle_failures(output);
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].method_name, "methodA");
        assert_eq!(failures[1].method_name, "methodB");
    }

    #[test]
    fn parse_maven_error_failures_test() {
        let output = r#"
[ERROR] Failures:
[ERROR]   AppTest.testDivide:42 expected:<4> but was:<3>
[ERROR]
"#;
        let failures = parse_maven_error_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].method_name, "testDivide");
    }

    #[test]
    fn strip_maven_prefix_test() {
        assert_eq!(strip_maven_prefix("[INFO] Hello"), "Hello");
        assert_eq!(strip_maven_prefix("[ERROR] Fail"), "Fail");
        assert_eq!(strip_maven_prefix("[WARNING] Warn"), "Warn");
        assert_eq!(strip_maven_prefix("No prefix"), "No prefix");
    }

    #[test]
    fn enrich_with_errors_test() {
        let mut suites = vec![TestSuite {
            name: "com.example.AppTest".into(),
            tests: vec![
                TestCase {
                    name: "testAdd".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
                TestCase {
                    name: "testDivide".into(),
                    status: TestStatus::Failed,
                    duration: Duration::from_millis(0),
                    error: None,
                },
            ],
        }];
        let failures = vec![JavaTestFailure {
            test_name: "com.example.AppTest.testDivide".into(),
            method_name: "testDivide".into(),
            message: "expected 4 got 3".into(),
            stack_trace: Some("at com.example.AppTest.testDivide(AppTest.java:42)".into()),
        }];
        enrich_with_errors(&mut suites, &failures);
        assert!(suites[0].tests[0].error.is_none());
        let err = suites[0].tests[1].error.as_ref().unwrap();
        assert_eq!(err.message, "expected 4 got 3");
        assert!(err.location.is_some());
    }

    #[test]
    fn truncate_java_message_test() {
        assert_eq!(truncate_java_message("short", 100), "short");
        let long = "x".repeat(600);
        let truncated = truncate_java_message(&long, 500);
        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.len(), 503);
    }

    #[test]
    fn parse_surefire_xml_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.MathTest" tests="3" failures="1" errors="0" skipped="0" time="0.05">
  <testcase name="testAdd" classname="com.example.MathTest" time="0.01"/>
  <testcase name="testSub" classname="com.example.MathTest" time="0.02"/>
  <testcase name="testDiv" classname="com.example.MathTest" time="0.02">
    <failure message="expected:&lt;4&gt; but was:&lt;3&gt;" type="AssertionError">
      stack trace here
    </failure>
  </testcase>
</testsuite>"#;
        let suite = parse_single_surefire_xml(xml).unwrap();
        assert_eq!(suite.name, "com.example.MathTest");
        assert_eq!(suite.tests.len(), 3);
        assert_eq!(suite.tests[0].status, TestStatus::Passed);
        assert_eq!(suite.tests[0].name, "testAdd");
        assert_eq!(suite.tests[2].status, TestStatus::Failed);
        let err = suite.tests[2].error.as_ref().unwrap();
        assert!(err.message.contains("expected:<4>"));
    }

    #[test]
    fn parse_surefire_xml_with_skipped() {
        let xml = r#"<testsuite name="Test" tests="2" failures="0" errors="0" skipped="1" time="0.01">
  <testcase name="testA" classname="Test" time="0.005"/>
  <testcase name="testB" classname="Test" time="0.005">
    <skipped/>
  </testcase>
</testsuite>"#;
        let suite = parse_single_surefire_xml(xml).unwrap();
        assert_eq!(suite.tests.len(), 2);
        assert_eq!(suite.tests[0].status, TestStatus::Passed);
        assert_eq!(suite.tests[1].status, TestStatus::Skipped);
    }

    #[test]
    fn parse_surefire_xml_with_error() {
        let xml = r#"<testsuite name="Test" tests="1" failures="0" errors="1" time="0.01">
  <testcase name="testBroken" classname="Test" time="0.001">
    <error message="NullPointerException" type="java.lang.NullPointerException">
      at Test.testBroken(Test.java:5)
    </error>
  </testcase>
</testsuite>"#;
        let suite = parse_single_surefire_xml(xml).unwrap();
        assert_eq!(suite.tests[0].status, TestStatus::Failed);
        assert!(
            suite.tests[0]
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("NullPointerException")
        );
    }

    #[test]
    fn parse_surefire_xml_empty() {
        assert!(parse_single_surefire_xml("<testsuite name=\"Test\"></testsuite>").is_none());
    }

    #[test]
    fn extract_xml_attr_test() {
        assert_eq!(
            extract_xml_attr(r#"<tag name="value">"#, "tag", "name"),
            Some("value".into())
        );
        assert_eq!(
            extract_xml_attr(r#"<tag foo="bar" baz="qux">"#, "tag", "baz"),
            Some("qux".into())
        );
        assert!(extract_xml_attr(r#"<tag>"#, "tag", "name").is_none());
    }

    #[test]
    fn xml_unescape_test() {
        assert_eq!(
            xml_unescape("expected:&lt;4&gt; but was:&lt;3&gt;"),
            "expected:<4> but was:<3>"
        );
        assert_eq!(xml_unescape("&amp;&quot;&apos;"), "&\"'");
    }

    #[test]
    fn parse_java_failures_integration() {
        let output = r#"
[INFO] -------------------------------------------------------
[INFO]  T E S T S
[INFO] -------------------------------------------------------
[INFO] Running com.example.AppTest
[ERROR] Tests run: 3, Failures: 1, Errors: 0, Skipped: 0

Failed tests:
  testDivide(com.example.AppTest): expected:<4> but was:<3>

[INFO] BUILD FAILURE
"#;
        let adapter = JavaAdapter::new();
        let result = adapter.parse_output(output, "", 1);
        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_failed(), 1);
    }
}
