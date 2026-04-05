//! History display and formatting.
//!
//! Formats test history data for terminal output.

use std::fmt::Write;

use super::{DurationTrend, FlakyTest, SlowTest, TestHistory, TestTrend};

/// Format a summary of recent test runs.
pub fn format_recent_runs(history: &TestHistory, n: usize) -> String {
    let runs = history.recent_runs(n);
    let mut out = String::with_capacity(1024);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Recent Test Runs");
    let _ = writeln!(out, "  ═══════════════════════════════════════");

    if runs.is_empty() {
        let _ = writeln!(out, "  No test runs recorded yet.");
        return out;
    }

    let _ = writeln!(
        out,
        "  {:<22}  {:>5}  {:>5}  {:>5}  {:>5}  {:>8}  Status",
        "Timestamp", "Total", "Pass", "Fail", "Skip", "Duration"
    );
    let _ = writeln!(
        out,
        "  {:<22}  {:>5}  {:>5}  {:>5}  {:>5}  {:>8}  ──────",
        "──────────────────────", "─────", "─────", "─────", "─────", "────────"
    );

    for run in runs.iter().rev() {
        let status = if run.failed == 0 { "✅" } else { "❌" };
        let duration = format_duration_ms(run.duration_ms);
        let _ = writeln!(
            out,
            "  {:<22}  {:>5}  {:>5}  {:>5}  {:>5}  {:>8}  {}",
            &run.timestamp[..19.min(run.timestamp.len())],
            run.total,
            run.passed,
            run.failed,
            run.skipped,
            duration,
            status,
        );
    }

    let _ = writeln!(out);
    out
}

/// Format flaky test report.
pub fn format_flaky_tests(flaky: &[FlakyTest]) -> String {
    let mut out = String::with_capacity(512);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Flaky Tests");
    let _ = writeln!(out, "  ═══════════════════════════════════════");

    if flaky.is_empty() {
        let _ = writeln!(out, "  No flaky tests detected! ✅");
        return out;
    }

    let _ = writeln!(
        out,
        "  {:<40}  {:>8}  {:>6}  {:>5}  Recent",
        "Test", "PassRate", "Runs", "Fails"
    );
    let _ = writeln!(
        out,
        "  {:<40}  {:>8}  {:>6}  {:>5}  ──────────",
        "────────────────────────────────────────", "────────", "──────", "─────"
    );

    for test in flaky {
        let name = if test.name.len() > 40 {
            let start = test
                .name
                .ceil_char_boundary(test.name.len().saturating_sub(39));
            format!("…{}", &test.name[start..])
        } else {
            test.name.clone()
        };

        let _ = writeln!(
            out,
            "  {:<40}  {:>7.1}%  {:>6}  {:>5}  {}",
            name,
            test.pass_rate * 100.0,
            test.total_runs,
            test.failures,
            test.recent_pattern,
        );
    }

    let _ = writeln!(out);
    out
}

/// Format slow test trends.
pub fn format_slow_tests(slow: &[SlowTest]) -> String {
    let mut out = String::with_capacity(512);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Slow Test Trends");
    let _ = writeln!(out, "  ═══════════════════════════════════════");

    if slow.is_empty() {
        let _ = writeln!(out, "  No significant duration trends detected.");
        return out;
    }

    let _ = writeln!(
        out,
        "  {:<40}  {:>8}  {:>8}  {:>8}  Trend",
        "Test", "Avg", "Latest", "Change"
    );
    let _ = writeln!(
        out,
        "  {:<40}  {:>8}  {:>8}  {:>8}  ─────",
        "────────────────────────────────────────", "────────", "────────", "────────"
    );

    for test in slow.iter().take(20) {
        let name = if test.name.len() > 40 {
            let start = test
                .name
                .ceil_char_boundary(test.name.len().saturating_sub(39));
            format!("…{}", &test.name[start..])
        } else {
            test.name.clone()
        };

        let trend_icon = match test.trend {
            DurationTrend::Faster => "↓ ✅",
            DurationTrend::Slower => "↑ ⚠",
            DurationTrend::Stable => "→",
        };

        let _ = writeln!(
            out,
            "  {:<40}  {:>8}  {:>8}  {:>+7.1}%  {}",
            name,
            format_duration_ms(test.avg_duration.as_millis() as u64),
            format_duration_ms(test.latest_duration.as_millis() as u64),
            test.change_pct,
            trend_icon,
        );
    }

    let _ = writeln!(out);
    out
}

/// Format trend data for a specific test.
pub fn format_test_trend(test_name: &str, trend: &[TestTrend]) -> String {
    let mut out = String::with_capacity(256);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Trend: {test_name}");
    let _ = writeln!(out, "  ─────────────────────────────────────");

    if trend.is_empty() {
        let _ = writeln!(out, "  No data available for this test.");
        return out;
    }

    // Show sparkline of durations
    let durations: Vec<u64> = trend.iter().map(|t| t.duration_ms).collect();
    let sparkline = make_sparkline(&durations);
    let _ = writeln!(out, "  Duration: {sparkline}");
    let _ = writeln!(out);

    let _ = writeln!(out, "  {:<22}  {:>8}  Status", "Timestamp", "Duration");
    let _ = writeln!(
        out,
        "  {:<22}  {:>8}  ──────",
        "──────────────────────", "────────"
    );

    for point in trend.iter().rev().take(20) {
        let status = match point.status.as_str() {
            "passed" => "✅",
            "failed" => "❌",
            "skipped" => "⏭️",
            _ => "?",
        };
        let _ = writeln!(
            out,
            "  {:<22}  {:>8}  {}",
            &point.timestamp[..19.min(point.timestamp.len())],
            format_duration_ms(point.duration_ms),
            status,
        );
    }

    let _ = writeln!(out);
    out
}

/// Format a quick stats summary.
pub fn format_stats_summary(history: &TestHistory) -> String {
    let mut out = String::with_capacity(512);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Test Health Dashboard");
    let _ = writeln!(out, "  ═══════════════════════════════════════");

    let _ = writeln!(out, "  Total Runs:    {}", history.run_count());
    let _ = writeln!(out, "  Pass Rate:     {:.1}%", history.pass_rate(30));
    let _ = writeln!(
        out,
        "  Avg Duration:  {}",
        format_duration_ms(history.avg_duration(30).as_millis() as u64)
    );

    // Sparkline of recent pass rates
    let recent = history.recent_runs(30);
    if !recent.is_empty() {
        let pass_rates: Vec<u64> = recent
            .iter()
            .map(|r| {
                if r.total > 0 {
                    (r.passed as f64 / r.total as f64 * 100.0) as u64
                } else {
                    0
                }
            })
            .collect();
        let sparkline = make_sparkline(&pass_rates);
        let _ = writeln!(out, "  Pass Rate:     {sparkline}");
    }

    let _ = writeln!(out);
    out
}

/// Create a sparkline from a series of values.
fn make_sparkline(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let min = *values.iter().min().unwrap_or(&0);
    let max = *values.iter().max().unwrap_or(&1);
    let range = if max == min { 1 } else { max - min };

    values
        .iter()
        .map(|&v| {
            let idx = ((v - min) as f64 / range as f64 * 7.0).round() as usize;
            chars[idx.min(7)]
        })
        .collect()
}

/// Format milliseconds as a human-readable duration.
fn format_duration_ms(ms: u64) -> String {
    if ms == 0 {
        "<1ms".to_string()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let minutes = ms / 60000;
        let seconds = (ms % 60000) / 1000;
        format!("{minutes}m{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};
    use crate::history::RunRecord;
    use std::time::Duration;

    fn make_result(passed: usize, failed: usize) -> TestRunResult {
        let mut tests = Vec::new();
        for i in 0..passed {
            tests.push(TestCase {
                name: format!("pass_{i}"),
                status: TestStatus::Passed,
                duration: Duration::from_millis(10),
                error: None,
            });
        }
        for i in 0..failed {
            tests.push(TestCase {
                name: format!("fail_{i}"),
                status: TestStatus::Failed,
                duration: Duration::from_millis(5),
                error: None,
            });
        }
        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".into(),
                tests,
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: if failed > 0 { 1 } else { 0 },
        }
    }

    fn populated_history() -> TestHistory {
        let mut h = TestHistory::new_in_memory();
        for _ in 0..5 {
            h.runs.push(RunRecord::from_result(&make_result(5, 0)));
        }
        h.runs.push(RunRecord::from_result(&make_result(4, 1)));
        h
    }

    #[test]
    fn recent_runs_format() {
        let h = populated_history();
        let output = format_recent_runs(&h, 3);
        assert!(output.contains("Recent Test Runs"));
        assert!(output.contains("Total"));
    }

    #[test]
    fn recent_runs_empty() {
        let h = TestHistory::new_in_memory();
        let output = format_recent_runs(&h, 5);
        assert!(output.contains("No test runs recorded"));
    }

    #[test]
    fn flaky_format_empty() {
        let output = format_flaky_tests(&[]);
        assert!(output.contains("No flaky tests"));
    }

    #[test]
    fn flaky_format_with_tests() {
        let flaky = vec![FlakyTest {
            name: "suite::test_oauth".into(),
            pass_rate: 0.72,
            total_runs: 25,
            failures: 7,
            recent_pattern: "PPFPFPPFPF".into(),
        }];
        let output = format_flaky_tests(&flaky);
        assert!(output.contains("test_oauth"));
        assert!(output.contains("72.0%"));
        assert!(output.contains("PPFPFPPFPF"));
    }

    #[test]
    fn slow_format_empty() {
        let output = format_slow_tests(&[]);
        assert!(output.contains("No significant duration"));
    }

    #[test]
    fn slow_format_with_tests() {
        let slow = vec![SlowTest {
            name: "suite::test_migration".into(),
            avg_duration: Duration::from_millis(2100),
            latest_duration: Duration::from_millis(3400),
            trend: DurationTrend::Slower,
            change_pct: 62.0,
        }];
        let output = format_slow_tests(&slow);
        assert!(output.contains("test_migration"));
        assert!(output.contains("⚠"));
    }

    #[test]
    fn test_trend_format() {
        let trend = vec![
            TestTrend {
                timestamp: "2024-01-01T00:00:00Z".into(),
                status: "passed".into(),
                duration_ms: 100,
            },
            TestTrend {
                timestamp: "2024-01-02T00:00:00Z".into(),
                status: "failed".into(),
                duration_ms: 150,
            },
        ];
        let output = format_test_trend("suite::test_login", &trend);
        assert!(output.contains("test_login"));
        assert!(output.contains("Duration:"));
    }

    #[test]
    fn test_trend_empty() {
        let output = format_test_trend("missing_test", &[]);
        assert!(output.contains("No data available"));
    }

    #[test]
    fn stats_summary() {
        let h = populated_history();
        let output = format_stats_summary(&h);
        assert!(output.contains("Test Health Dashboard"));
        assert!(output.contains("Total Runs"));
        assert!(output.contains("Pass Rate"));
    }

    #[test]
    fn sparkline_basic() {
        let spark = make_sparkline(&[0, 50, 100]);
        assert_eq!(spark.chars().count(), 3);
        assert!(spark.contains('▁'));
        assert!(spark.contains('█'));
    }

    #[test]
    fn sparkline_empty() {
        assert!(make_sparkline(&[]).is_empty());
    }

    #[test]
    fn sparkline_single() {
        let spark = make_sparkline(&[42]);
        assert_eq!(spark.chars().count(), 1);
    }

    #[test]
    fn format_duration_ms_tests() {
        assert_eq!(format_duration_ms(0), "<1ms");
        assert_eq!(format_duration_ms(42), "42ms");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(65000), "1m5s");
    }
}
