use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use super::util::duration_from_secs_safe;
use super::{
    ConfidenceScore, DetectionResult, TestAdapter, TestCase, TestRunResult, TestStatus, TestSuite,
};

/// Build a Command to run a JS tool via the detected package manager.
/// npx: `npx <tool>`, bun: `bunx <tool>`, yarn/pnpm: `yarn <tool>` / `pnpm <tool>`
fn build_js_runner_cmd(pkg_manager: &str, tool: &str) -> Command {
    let mut cmd = Command::new(pkg_manager);
    match pkg_manager {
        "npx" => {
            cmd.arg(tool);
        }
        "bun" => {
            cmd.arg("x").arg(tool);
        }
        // yarn and pnpm can run local binaries directly
        _ => {
            cmd.arg(tool);
        }
    }
    cmd
}

pub struct JavaScriptAdapter;

impl Default for JavaScriptAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaScriptAdapter {
    pub fn new() -> Self {
        Self
    }

    fn detect_package_manager(project_dir: &Path) -> &'static str {
        if project_dir.join("bun.lockb").exists() || project_dir.join("bun.lock").exists() {
            "bun"
        } else if project_dir.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if project_dir.join("yarn.lock").exists() {
            "yarn"
        } else {
            "npx"
        }
    }

    fn detect_framework(project_dir: &Path) -> Option<&'static str> {
        let pkg_json = project_dir.join("package.json");
        if !pkg_json.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&pkg_json).ok()?;

        // Check for vitest config files first (highest priority)
        if project_dir.join("vitest.config.ts").exists()
            || project_dir.join("vitest.config.js").exists()
            || project_dir.join("vitest.config.mts").exists()
            || content.contains("\"vitest\"")
        {
            return Some("vitest");
        }

        // Check for bun test
        if project_dir.join("bunfig.toml").exists()
            && (project_dir.join("bun.lockb").exists() || project_dir.join("bun.lock").exists())
        {
            // bun has a built-in test runner
            if content.contains("\"bun:test\"") || !content.contains("\"jest\"") {
                return Some("bun");
            }
        }

        // Jest
        if project_dir.join("jest.config.ts").exists()
            || project_dir.join("jest.config.js").exists()
            || project_dir.join("jest.config.cjs").exists()
            || project_dir.join("jest.config.mjs").exists()
            || content.contains("\"jest\"")
        {
            return Some("jest");
        }

        // Mocha
        if project_dir.join(".mocharc.yml").exists()
            || project_dir.join(".mocharc.json").exists()
            || project_dir.join(".mocharc.js").exists()
            || content.contains("\"mocha\"")
        {
            return Some("mocha");
        }

        // AVA
        if project_dir.join("ava.config.js").exists()
            || project_dir.join("ava.config.cjs").exists()
            || project_dir.join("ava.config.mjs").exists()
            || content.contains("\"ava\"")
        {
            return Some("ava");
        }

        None
    }
}

impl TestAdapter for JavaScriptAdapter {
    fn name(&self) -> &str {
        "JavaScript/TypeScript"
    }

    fn check_runner(&self) -> Option<String> {
        // Check for any common JS runner
        for runner in ["npx", "bun", "yarn", "pnpm"] {
            if which::which(runner).is_ok() {
                return None;
            }
        }
        Some("node/npm".into())
    }

    fn detect(&self, project_dir: &Path) -> Option<DetectionResult> {
        let framework = Self::detect_framework(project_dir)?;

        let has_config = project_dir.join("vitest.config.ts").exists()
            || project_dir.join("vitest.config.js").exists()
            || project_dir.join("jest.config.ts").exists()
            || project_dir.join("jest.config.js").exists()
            || project_dir.join(".mocharc.yml").exists()
            || project_dir.join(".mocharc.json").exists();
        let has_lock = project_dir.join("package-lock.json").exists()
            || project_dir.join("yarn.lock").exists()
            || project_dir.join("pnpm-lock.yaml").exists()
            || project_dir.join("bun.lockb").exists()
            || project_dir.join("bun.lock").exists();
        let has_runner = ["npx", "bun", "yarn", "pnpm"]
            .iter()
            .any(|r| which::which(r).is_ok());

        let confidence = ConfidenceScore::base(0.50)
            .signal(0.15, has_config)
            .signal(0.10, project_dir.join("node_modules").is_dir())
            .signal(0.10, has_lock)
            .signal(0.07, has_runner)
            .finish();

        Some(DetectionResult {
            language: "JavaScript".into(),
            framework: framework.into(),
            confidence,
        })
    }

    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command> {
        let framework = Self::detect_framework(project_dir).unwrap_or("jest");
        let pkg_manager = Self::detect_package_manager(project_dir);

        let mut cmd;

        match framework {
            "vitest" => {
                cmd = build_js_runner_cmd(pkg_manager, "vitest");
                cmd.arg("run"); // non-watch mode
            }
            "jest" => {
                cmd = build_js_runner_cmd(pkg_manager, "jest");
            }
            "bun" => {
                cmd = Command::new("bun");
                cmd.arg("test");
            }
            "mocha" => {
                cmd = build_js_runner_cmd(pkg_manager, "mocha");
            }
            "ava" => {
                cmd = build_js_runner_cmd(pkg_manager, "ava");
            }
            _ => {
                cmd = build_js_runner_cmd(pkg_manager, "jest");
            }
        }

        for arg in extra_args {
            cmd.arg(arg);
        }

        cmd.current_dir(project_dir);
        Ok(cmd)
    }

    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult {
        let combined = strip_ansi(&format!("{}\n{}", stdout, stderr));
        let failure_messages = parse_jest_failures(&combined);
        let mut suites: Vec<TestSuite> = Vec::new();
        let mut current_suite = String::new();
        let mut current_tests: Vec<TestCase> = Vec::new();

        for line in combined.lines() {
            let trimmed = line.trim();

            // Jest/Vitest suite header: "PASS src/utils.test.ts" or "FAIL src/utils.test.ts"
            if trimmed.starts_with("PASS ") || trimmed.starts_with("FAIL ") {
                // Flush previous suite
                if !current_suite.is_empty() && !current_tests.is_empty() {
                    suites.push(TestSuite {
                        name: current_suite.clone(),
                        tests: std::mem::take(&mut current_tests),
                    });
                }
                current_suite = trimmed
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("tests")
                    .to_string();
                continue;
            }

            // Jest/Vitest/AVA test result lines
            // Jest/Vitest: "✓ should work (5 ms)" / "✕ should fail"
            // AVA: "✔ suite › test name" / "✘ [fail]: suite › test name Error"
            if trimmed.starts_with('✓')
                || trimmed.starts_with('✕')
                || trimmed.starts_with('○')
                || trimmed.starts_with("√")
                || trimmed.starts_with("×")
                || trimmed.starts_with('✔')
                || trimmed.starts_with('✘')
            {
                let status = if trimmed.starts_with('✓')
                    || trimmed.starts_with("√")
                    || trimmed.starts_with('✔')
                {
                    TestStatus::Passed
                } else if trimmed.starts_with('○') {
                    TestStatus::Skipped
                } else {
                    TestStatus::Failed
                };

                let rest = &trimmed[trimmed.char_indices().nth(1).map(|(i, _)| i).unwrap_or(1)..]
                    .trim_start();
                // AVA failure format: "[fail]: suite › test Error msg" — strip "[fail]: " prefix
                let rest = rest.strip_prefix("[fail]: ").unwrap_or(rest);
                let (name, duration) = parse_jest_test_line(rest);

                let error = if status == TestStatus::Failed {
                    failure_messages.get(&name).map(|msg| super::TestError {
                        message: msg.clone(),
                        location: None,
                    })
                } else {
                    None
                };

                current_tests.push(TestCase {
                    name,
                    status,
                    duration,
                    error,
                });
                continue;
            }

            // Vitest format: "  ✓ module > test name 5ms"
            if (trimmed.contains(" ✓ ")
                || trimmed.contains(" ✕ ")
                || trimmed.contains(" × ")
                || trimmed.contains(" ✔ ")
                || trimmed.contains(" ✘ "))
                && !trimmed.starts_with("Test")
            {
                let status = if trimmed.contains(" ✓ ") || trimmed.contains(" ✔ ") {
                    TestStatus::Passed
                } else {
                    TestStatus::Failed
                };

                let name = trimmed
                    .replace(" ✓ ", "")
                    .replace(" ✕ ", "")
                    .replace(" × ", "")
                    .replace(" ✔ ", "")
                    .replace(" ✘ ", "")
                    .trim()
                    .to_string();
                // Strip AVA "[fail]: " prefix
                let name = name
                    .strip_prefix("[fail]: ")
                    .map(|s| s.to_string())
                    .unwrap_or(name);

                let error = if status == TestStatus::Failed {
                    failure_messages.get(&name).map(|msg| super::TestError {
                        message: msg.clone(),
                        location: None,
                    })
                } else {
                    None
                };

                current_tests.push(TestCase {
                    name,
                    status,
                    duration: Duration::from_millis(0),
                    error,
                });
            }
        }

        // Flush last suite
        if !current_tests.is_empty() {
            let suite_name = if current_suite.is_empty() {
                "tests".into()
            } else {
                current_suite
            };
            suites.push(TestSuite {
                name: suite_name,
                tests: current_tests,
            });
        }

        // Fallback: parse summary line
        if suites.is_empty() {
            suites.push(parse_jest_summary(&combined, exit_code));
        } else {
            // If we parsed individual lines, but a summary line shows more tests,
            // prefer the summary (this handles vitest default output where ✓ lines are
            // file-level, not test-level)
            let summary = parse_jest_summary(&combined, exit_code);
            let inline_total: usize = suites.iter().map(|s| s.tests.len()).sum();
            let summary_total = summary.tests.len();
            if summary_total > inline_total && summary_total > 1 {
                suites = vec![summary];
            }
        }

        let duration = parse_jest_duration(&combined).unwrap_or(Duration::from_secs(0));

        TestRunResult {
            suites,
            duration,
            raw_exit_code: exit_code,
        }
    }
}

/// Parse "should work (5 ms)" → ("should work", Duration(5ms))
fn parse_jest_test_line(line: &str) -> (String, Duration) {
    let trimmed = line.trim();
    if let Some(paren_start) = trimmed.rfind('(')
        && let Some(paren_end) = trimmed.rfind(')')
    {
        let name = trimmed[..paren_start].trim().to_string();
        let timing = &trimmed[paren_start + 1..paren_end];
        let ms = timing
            .replace("ms", "")
            .replace("s", "")
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0);
        let duration = if timing.contains("ms") {
            Duration::from_millis(ms as u64)
        } else {
            duration_from_secs_safe(ms)
        };
        return (name, duration);
    }
    (trimmed.to_string(), Duration::from_millis(0))
}

fn parse_jest_summary(output: &str, exit_code: i32) -> TestSuite {
    let mut tests = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();

        // Jest format: "Tests:  X passed, Y failed, Z total"
        if trimmed.contains("Tests:") && trimmed.contains("total") {
            let after_label = trimmed.split("Tests:").nth(1).unwrap_or(trimmed);
            for part in after_label.split(',') {
                let part = part.trim();
                if let Some(n) = part
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    let status = if part.contains("passed") {
                        TestStatus::Passed
                    } else if part.contains("failed") {
                        TestStatus::Failed
                    } else if part.contains("skipped") || part.contains("todo") {
                        TestStatus::Skipped
                    } else {
                        continue;
                    };
                    for i in 0..n {
                        tests.push(TestCase {
                            name: format!(
                                "{}_{}",
                                if status == TestStatus::Passed {
                                    "test"
                                } else {
                                    "failed"
                                },
                                i + 1
                            ),
                            status: status.clone(),
                            duration: Duration::from_millis(0),
                            error: None,
                        });
                    }
                }
            }
            continue;
        }

        // Vitest format: "Tests  3575 passed (3575)" or "Tests  10 failed | 3565 passed (3575)"
        if (trimmed.starts_with("Tests") || trimmed.starts_with("Tests "))
            && !trimmed.contains(":")
            && (trimmed.contains("passed") || trimmed.contains("failed"))
        {
            // Split by | for multi-status: "10 failed | 3565 passed (3575)"
            let after_tests = trimmed.trim_start_matches("Tests").trim();
            for segment in after_tests.split('|') {
                let segment = segment.trim();
                // Extract "N status" pairs
                let words: Vec<&str> = segment.split_whitespace().collect();
                for w in words.windows(2) {
                    if let Ok(n) = w[0].parse::<usize>() {
                        let status_word = w[1].trim_end_matches(')');
                        let status = if status_word.contains("passed") {
                            TestStatus::Passed
                        } else if status_word.contains("failed") {
                            TestStatus::Failed
                        } else if status_word.contains("skipped") || status_word.contains("todo") {
                            TestStatus::Skipped
                        } else {
                            continue;
                        };
                        for i in 0..n {
                            tests.push(TestCase {
                                name: format!(
                                    "{}_{}",
                                    if status == TestStatus::Passed {
                                        "test"
                                    } else {
                                        "failed"
                                    },
                                    tests.len() + i + 1
                                ),
                                status: status.clone(),
                                duration: Duration::from_millis(0),
                                error: None,
                            });
                        }
                    }
                }
            }
            continue;
        }

        // AVA format: "30 tests failed" or "5 tests passed" or "2 known failures"
        if (trimmed.contains("tests passed")
            || trimmed.contains("tests failed")
            || trimmed.contains("test passed")
            || trimmed.contains("test failed"))
            && let Some(n) = trimmed
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<usize>().ok())
        {
            let status = if trimmed.contains("passed") {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            for i in 0..n {
                tests.push(TestCase {
                    name: format!(
                        "{}_{}",
                        if status == TestStatus::Passed {
                            "test"
                        } else {
                            "failed"
                        },
                        tests.len() + i + 1
                    ),
                    status: status.clone(),
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

    TestSuite {
        name: "tests".into(),
        tests,
    }
}

/// Parse Jest/Vitest failure blocks to extract error messages per test.
/// Jest shows errors like:
/// ```text
///   ● should multiply numbers
///
///     expect(received).toBe(expected)
///
///     Expected: 7
///     Received: 6
/// ```
fn parse_jest_failures(output: &str) -> std::collections::HashMap<String, String> {
    let mut failures = std::collections::HashMap::new();
    let lines: Vec<&str> = output.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "● test name" or "● describe › test name"
        if trimmed.starts_with('●') {
            let test_name = trimmed[trimmed
                .char_indices()
                .nth(1)
                .map(|(idx, _)| idx)
                .unwrap_or(1)..]
                .trim()
                .to_string();
            if !test_name.is_empty() {
                let mut error_lines = Vec::new();
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    // Next failure block or summary section
                    if l.starts_with('●')
                        || l.starts_with("Test Suites:")
                        || l.starts_with("Tests:")
                    {
                        break;
                    }
                    // Collect meaningful error lines (skip empty, skip code frame lines starting with |)
                    if !l.is_empty() && !l.starts_with('|') && !l.starts_with("at ") {
                        error_lines.push(l.to_string());
                    }
                    i += 1;
                }
                if !error_lines.is_empty() {
                    // Take first few lines to keep message concise
                    let msg = error_lines
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | ");
                    // The test name in the ● block might be "describe › test name"
                    // but in results it's just "test name". Store under last segment too.
                    let short_name = test_name
                        .split(" › ")
                        .last()
                        .unwrap_or(&test_name)
                        .to_string();
                    failures.insert(test_name.clone(), msg.clone());
                    if short_name != test_name {
                        failures.insert(short_name, msg);
                    }
                }
                continue;
            }
        }
        i += 1;
    }
    failures
}

fn parse_jest_duration(output: &str) -> Option<Duration> {
    for line in output.lines() {
        let trimmed = line.trim();
        // Jest: "Time:  1.234 s" or "Time:  123 ms"
        if trimmed.contains("Time:") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if let Ok(n) = part.parse::<f64>()
                    && let Some(unit) = parts.get(i + 1)
                {
                    if unit.starts_with('s') {
                        return Some(duration_from_secs_safe(n));
                    } else if unit.starts_with("ms") {
                        return Some(Duration::from_millis(n as u64));
                    }
                }
            }
        }
        // Vitest: "Duration  30.18s (transform 24.34s, ...)"
        if trimmed.starts_with("Duration")
            && !trimmed.contains(":")
            && let Some(dur_str) = trimmed
                .strip_prefix("Duration")
                .and_then(|s| s.split_whitespace().next())
        {
            if let Some(secs) = dur_str
                .strip_suffix('s')
                .and_then(|s| s.parse::<f64>().ok())
            {
                return Some(duration_from_secs_safe(secs));
            } else if let Some(ms) = dur_str
                .strip_suffix("ms")
                .and_then(|s| s.parse::<f64>().ok())
            {
                return Some(Duration::from_millis(ms as u64));
            }
        }
    }
    None
}

/// Strip ANSI escape codes from text. Handles CSI sequences like \x1b[32m.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip CSI sequence: ESC [ ... (letter)
            if let Some(next) = chars.next()
                && next == '['
            {
                // Consume until a letter (A-Z, a-z) terminates the sequence
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // else: non-CSI escape, skip the next char
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jest_verbose_output() {
        let stdout = r#"
PASS src/utils.test.ts
  ✓ should add numbers (3 ms)
  ✓ should subtract numbers (1 ms)
  ✕ should multiply numbers (2 ms)

  ● should multiply numbers

    expect(received).toBe(expected)

    Expected: 7
    Received: 6

Test Suites: 1 passed, 1 total
Tests:       2 passed, 1 failed, 3 total
Time:        1.234 s
"#;
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert!(!result.is_success());

        // Verify error message was captured
        let failed = &result.suites[0].failures();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].error.is_some());
        assert!(
            failed[0]
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("expect(received).toBe(expected)")
        );
    }

    #[test]
    fn parse_jest_all_pass() {
        let stdout = r#"
PASS src/math.test.ts
  ✓ test_one (5 ms)
  ✓ test_two (2 ms)

Tests:       2 passed, 2 total
Time:        0.456 s
"#;
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 2);
        assert!(result.is_success());
        assert_eq!(result.duration, Duration::from_millis(456));
    }

    #[test]
    fn parse_jest_summary_fallback() {
        let stdout = "Tests:  5 passed, 2 failed, 7 total\nTime:        3.21 s\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_passed(), 5);
        assert_eq!(result.total_failed(), 2);
    }

    #[test]
    fn parse_jest_test_line_with_duration() {
        let (name, dur) = parse_jest_test_line(" should add numbers (5 ms)");
        assert_eq!(name, "should add numbers");
        assert_eq!(dur, Duration::from_millis(5));
    }

    #[test]
    fn parse_jest_test_line_no_duration() {
        let (name, dur) = parse_jest_test_line(" should add numbers");
        assert_eq!(name, "should add numbers");
        assert_eq!(dur, Duration::from_millis(0));
    }

    #[test]
    fn parse_jest_duration_seconds() {
        assert_eq!(
            parse_jest_duration("Time:        1.234 s"),
            Some(Duration::from_millis(1234))
        );
    }

    #[test]
    fn parse_jest_duration_ms() {
        assert_eq!(
            parse_jest_duration("Time:        456 ms"),
            Some(Duration::from_millis(456))
        );
    }

    #[test]
    fn detect_vitest_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"vitest":"^1.0"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("vitest.config.ts"), "export default {}").unwrap();
        let adapter = JavaScriptAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "vitest");
    }

    #[test]
    fn detect_jest_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"jest":"^29"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("jest.config.js"), "module.exports = {}").unwrap();
        let adapter = JavaScriptAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "jest");
    }

    #[test]
    fn detect_no_js() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), "print('hello')\n").unwrap();
        let adapter = JavaScriptAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn detect_bun_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bun.lockb"), "").unwrap();
        assert_eq!(JavaScriptAdapter::detect_package_manager(dir.path()), "bun");
    }

    #[test]
    fn detect_pnpm_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(
            JavaScriptAdapter::detect_package_manager(dir.path()),
            "pnpm"
        );
    }

    #[test]
    fn parse_jest_empty_output() {
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output("", "", 0);

        assert_eq!(result.total_tests(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn parse_jest_with_describe_blocks() {
        let stdout = r#"
PASS src/math.test.ts
  Math operations
    ✓ should add (2 ms)
    ✓ should subtract (1 ms)
  String operations
    ✕ should uppercase (3 ms)

  ● String operations › should uppercase

    expect(received).toBe(expected)

Tests:       2 passed, 1 failed, 3 total
Time:        0.789 s
"#;
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 3);
        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_jest_multiple_suites() {
        let stdout = r#"
PASS src/a.test.ts
  ✓ test_a1 (1 ms)
  ✓ test_a2 (1 ms)
FAIL src/b.test.ts
  ✓ test_b1 (1 ms)
  ✕ test_b2 (5 ms)

Tests:       3 passed, 1 failed, 4 total
Time:        1.0 s
"#;
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_tests(), 4);
        assert_eq!(result.suites.len(), 2);
        assert_eq!(result.suites[0].name, "src/a.test.ts");
        assert_eq!(result.suites[1].name, "src/b.test.ts");
    }

    #[test]
    fn parse_jest_skipped_tests() {
        let stdout = r#"
PASS src/utils.test.ts
  ✓ should work (2 ms)
  ○ skipped test

Tests:       1 passed, 1 skipped, 2 total
Time:        0.5 s
"#;
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_skipped(), 1);
        assert!(result.is_success());
    }

    #[test]
    fn detect_yarn_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        assert_eq!(
            JavaScriptAdapter::detect_package_manager(dir.path()),
            "yarn"
        );
    }

    #[test]
    fn detect_npx_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(JavaScriptAdapter::detect_package_manager(dir.path()), "npx");
    }

    #[test]
    fn detect_mocha_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"mocha":"^10"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join(".mocharc.yml"), "").unwrap();
        let adapter = JavaScriptAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "mocha");
    }

    #[test]
    fn detect_no_framework_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.js"), "console.log('hi')").unwrap();
        let adapter = JavaScriptAdapter::new();
        assert!(adapter.detect(dir.path()).is_none());
    }

    #[test]
    fn parse_ava_output() {
        let stdout = "  ✔ body-size › returns 0 for null\n  ✔ body-size › returns correct size\n  ✘ [fail]: browser › request fails Rejected promise\n\n  1 test failed\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_passed(), 2);
        assert_eq!(result.total_failed(), 1);
        assert_eq!(result.total_tests(), 3);
    }

    #[test]
    fn parse_ava_checkmark_chars() {
        // ✔ = U+2714, ✘ = U+2718 (different from Jest ✓/✕)
        let stdout = "✔ test_one\n✘ test_two\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_passed(), 1);
        assert_eq!(result.total_failed(), 1);
    }

    #[test]
    fn parse_vitest_summary_format() {
        // Vitest: "Tests  3575 passed (3575)"
        let stdout = " Test Files  323 passed (323)\n      Tests  3575 passed (3575)\n   Start at  12:24:03\n   Duration  30.18s\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 0);

        assert_eq!(result.total_passed(), 3575);
        assert!(result.is_success());
    }

    #[test]
    fn parse_vitest_mixed_summary() {
        let stdout = "      Tests  10 failed | 3565 passed (3575)\n   Duration  30.18s\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_passed(), 3565);
        assert_eq!(result.total_failed(), 10);
        assert_eq!(result.total_tests(), 3575);
    }

    #[test]
    fn parse_vitest_duration_format() {
        assert_eq!(
            parse_jest_duration("   Duration  30.18s (transform 24.34s, setup 16.70s)"),
            Some(Duration::from_millis(30180))
        );
    }

    #[test]
    fn parse_ava_summary_fallback() {
        let stdout = "  30 tests failed\n  2 known failures\n";
        let adapter = JavaScriptAdapter::new();
        let result = adapter.parse_output(stdout, "", 1);

        assert_eq!(result.total_failed(), 30);
    }

    #[test]
    fn detect_ava_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"ava":"^6"}}"#,
        )
        .unwrap();
        let adapter = JavaScriptAdapter::new();
        let det = adapter.detect(dir.path()).unwrap();
        assert_eq!(det.framework, "ava");
    }
}
