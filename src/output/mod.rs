use std::io::Write;
use std::time::Duration;

use colored::Colorize;

use crate::adapters::util::xml_escape;
use crate::adapters::{TestRunResult, TestStatus};
use crate::detection::DetectedProject;

pub fn print_detection(detected: &DetectedProject) {
    println!(
        "  {} {} ({}) [confidence: {:.0}%]",
        "▸".bold(),
        detected.detection.language.bold(),
        detected.detection.framework.dimmed(),
        detected.detection.confidence * 100.0,
    );
}

pub fn print_header(adapter_name: &str, detected: &DetectedProject) {
    println!();
    println!(
        "{} {} {}",
        "testx".bold().cyan(),
        "·".dimmed(),
        format!("{} ({})", adapter_name, detected.detection.framework).white(),
    );
    println!("{}", "─".repeat(60).dimmed());
}

pub fn print_results(result: &TestRunResult) {
    for suite in &result.suites {
        println!();
        let suite_icon = if suite.is_passed() {
            "✓".green()
        } else {
            "✗".red()
        };
        println!("  {} {}", suite_icon, suite.name.bold().underline());

        for test in &suite.tests {
            let (icon, name_colored) = match test.status {
                TestStatus::Passed => ("✓".green(), test.name.green()),
                TestStatus::Failed => ("✗".red(), test.name.red()),
                TestStatus::Skipped => ("○".yellow(), test.name.yellow()),
            };

            let duration_str = format_duration(test.duration);
            if test.duration.as_millis() > 0 {
                println!("    {} {} {}", icon, name_colored, duration_str.dimmed());
            } else {
                println!("    {} {}", icon, name_colored);
            }

            // Print error details if present
            if let Some(err) = &test.error {
                println!("      {} {}", "→".red(), err.message.red());
                if let Some(loc) = &err.location {
                    println!("        {}", loc.dimmed());
                }
            }
        }
    }

    println!();
    println!("{}", "─".repeat(60).dimmed());

    // Print failure summary if there are failures
    if !result.is_success() {
        print_failure_summary(result);
    }

    print_summary(result);
}

fn print_failure_summary(result: &TestRunResult) {
    let mut has_failures = false;
    for suite in &result.suites {
        let failures = suite.failures();
        if failures.is_empty() {
            continue;
        }
        if !has_failures {
            println!("  {} {}", "✗".red().bold(), "Failures:".red().bold());
            has_failures = true;
        }
        for tc in failures {
            println!(
                "    {} {} :: {}",
                "→".red(),
                suite.name.dimmed(),
                tc.name.red()
            );
            if let Some(err) = &tc.error {
                println!("      {}", err.message.dimmed());
            }
        }
    }
    if has_failures {
        println!();
    }
}

fn print_summary(result: &TestRunResult) {
    let total = result.total_tests();
    let passed = result.total_passed();
    let failed = result.total_failed();
    let skipped = result.total_skipped();

    let status_line = if result.is_success() {
        "PASS".green().bold()
    } else {
        "FAIL".red().bold()
    };

    let mut parts = Vec::new();
    if passed > 0 {
        parts.push(format!("{} passed", passed).green().to_string());
    }
    if failed > 0 {
        parts.push(format!("{} failed", failed).red().to_string());
    }
    if skipped > 0 {
        parts.push(format!("{} skipped", skipped).yellow().to_string());
    }

    println!(
        "  {} {} ({} total) in {}",
        status_line,
        parts.join(", "),
        total,
        format_duration(result.duration),
    );
    println!();
}

pub fn print_slowest_tests(result: &TestRunResult, count: usize) {
    let slowest = result.slowest_tests(count);
    if slowest.is_empty() || slowest.iter().all(|(_, tc)| tc.duration.as_millis() == 0) {
        return;
    }

    println!("  {} {}", "⏱".dimmed(), "Slowest tests:".dimmed());
    for (_suite, tc) in slowest {
        if tc.duration.as_millis() > 0 {
            println!(
                "    {} {}",
                format_duration(tc.duration).yellow(),
                tc.name.dimmed(),
            );
        }
    }
    println!();
}

/// Format a Duration as human-readable
fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        return String::new();
    }
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

/// Print results as JSON to stdout
pub fn print_json(result: &TestRunResult) {
    let json = serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".into());
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", json);
}

/// Print raw output from the test runner (useful for debugging failures)
pub fn print_raw_output(stdout: &str, stderr: &str) {
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    if stdout.is_empty() && stderr.is_empty() {
        return;
    }
    println!("  {} {}", "▾".dimmed(), "Raw output:".dimmed());
    println!("{}", "─".repeat(60).dimmed());
    if !stdout.is_empty() {
        println!("{}", stdout);
    }
    if !stderr.is_empty() {
        println!("{}", stderr);
    }
    println!("{}", "─".repeat(60).dimmed());
    println!();
}

/// Print results as JUnit XML (compatible with CI tools)
pub fn print_junit_xml(result: &TestRunResult) {
    use std::io::Write;

    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    buf.push_str(&format!(
        "<testsuites tests=\"{}\" failures=\"{}\" time=\"{:.3}\">\n",
        result.total_tests(),
        result.total_failed(),
        result.duration.as_secs_f64(),
    ));

    for suite in &result.suites {
        buf.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" skipped=\"{}\">\n",
            xml_escape(&suite.name),
            suite.tests.len(),
            suite.failed(),
            suite.skipped(),
        ));

        for test in &suite.tests {
            let time = format!("{:.3}", test.duration.as_secs_f64());
            match test.status {
                TestStatus::Passed => {
                    buf.push_str(&format!(
                        "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\"/>\n",
                        xml_escape(&test.name),
                        xml_escape(&suite.name),
                        time,
                    ));
                }
                TestStatus::Failed => {
                    buf.push_str(&format!(
                        "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\">\n",
                        xml_escape(&test.name),
                        xml_escape(&suite.name),
                        time,
                    ));
                    let msg = test
                        .error
                        .as_ref()
                        .map(|e| e.message.as_str())
                        .unwrap_or("Test failed");
                    buf.push_str(&format!(
                        "      <failure message=\"{}\" type=\"AssertionError\">{}</failure>\n",
                        xml_escape(msg),
                        xml_escape(msg),
                    ));
                    buf.push_str("    </testcase>\n");
                }
                TestStatus::Skipped => {
                    buf.push_str(&format!(
                        "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\">\n",
                        xml_escape(&test.name),
                        xml_escape(&suite.name),
                        time,
                    ));
                    buf.push_str("      <skipped/>\n");
                    buf.push_str("    </testcase>\n");
                }
            }
        }

        buf.push_str("  </testsuite>\n");
    }

    buf.push_str("</testsuites>\n");

    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(buf.as_bytes());
}

/// Print results in TAP (Test Anything Protocol) format
pub fn print_tap(result: &TestRunResult) {
    use std::io::Write;

    let total = result.total_tests();
    let mut stdout = std::io::stdout().lock();

    macro_rules! tap_write {
        ($($arg:tt)*) => {
            if writeln!(stdout, $($arg)*).is_err() {
                return;
            }
        }
    }

    tap_write!("TAP version 13");
    tap_write!("1..{total}");

    let mut n = 0;
    for suite in &result.suites {
        for test in &suite.tests {
            n += 1;
            let full_name = format!("{} - {}", suite.name, test.name);
            match test.status {
                TestStatus::Passed => {
                    tap_write!("ok {n} {full_name}");
                }
                TestStatus::Failed => {
                    tap_write!("not ok {n} {full_name}");
                    if let Some(err) = &test.error {
                        tap_write!("  ---");
                        tap_write!("  message: {}", err.message);
                        if let Some(loc) = &err.location {
                            tap_write!("  at: {loc}");
                        }
                        tap_write!("  ...");
                    }
                }
                TestStatus::Skipped => {
                    tap_write!("ok {n} {full_name} # SKIP");
                }
            }
        }
    }
}
