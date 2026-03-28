//! Test history and trend tracking.
//!
//! Stores test run results over time for trend analysis,
//! flaky test detection, and performance monitoring.
//! Uses a simple JSON-based file store (no external DB dependency).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::{TestRunResult, TestStatus};
use crate::error::TestxError;

pub mod analytics;
pub mod display;

/// Test history store backed by JSON files.
pub struct TestHistory {
    /// Directory where history files are stored
    data_dir: PathBuf,
    /// In-memory run records
    runs: Vec<RunRecord>,
    /// Maximum number of runs to keep
    max_runs: usize,
}

/// A single recorded test run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunRecord {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Total number of tests
    pub total: usize,
    /// Number of passed tests
    pub passed: usize,
    /// Number of failed tests
    pub failed: usize,
    /// Number of skipped tests
    pub skipped: usize,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Exit code
    pub exit_code: i32,
    /// Individual test results (name -> status + duration_ms)
    pub tests: Vec<TestRecord>,
}

/// A single test case record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestRecord {
    /// Full test name (suite::test)
    pub name: String,
    /// Test status
    pub status: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// A test that has been detected as flaky.
#[derive(Debug, Clone)]
pub struct FlakyTest {
    /// Test name
    pub name: String,
    /// Pass rate (0.0 - 1.0)
    pub pass_rate: f64,
    /// Number of runs analyzed
    pub total_runs: usize,
    /// Number of failures
    pub failures: usize,
    /// Number of recent consecutive results (P/F)
    pub recent_pattern: String,
}

/// A test that is getting slower over time.
#[derive(Debug, Clone)]
pub struct SlowTest {
    /// Test name
    pub name: String,
    /// Average duration over the period
    pub avg_duration: Duration,
    /// Most recent duration
    pub latest_duration: Duration,
    /// Duration trend
    pub trend: DurationTrend,
    /// Percentage change from average
    pub change_pct: f64,
}

/// Duration trend direction.
#[derive(Debug, Clone, PartialEq)]
pub enum DurationTrend {
    Faster,
    Slower,
    Stable,
}

/// Trend data point for a specific test.
#[derive(Debug, Clone)]
pub struct TestTrend {
    /// Timestamp
    pub timestamp: String,
    /// Status at this point
    pub status: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl TestHistory {
    /// Open or create a history store in the given directory.
    pub fn open(dir: &Path) -> crate::error::Result<Self> {
        let data_dir = dir.join(".testx");
        let history_file = data_dir.join("history.json");

        let runs = if history_file.exists() {
            let content =
                std::fs::read_to_string(&history_file).map_err(|e| TestxError::HistoryError {
                    message: format!("Failed to read history: {e}"),
                })?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            data_dir,
            runs,
            max_runs: 500,
        })
    }

    /// Create a new in-memory history (for testing).
    pub fn new_in_memory() -> Self {
        Self {
            data_dir: PathBuf::from("/tmp/testx-history"),
            runs: Vec::new(),
            max_runs: 500,
        }
    }

    /// Record a test run result.
    pub fn record(&mut self, result: &TestRunResult) -> crate::error::Result<()> {
        let record = RunRecord::from_result(result);
        self.runs.push(record);

        // Prune if over limit
        if self.runs.len() > self.max_runs {
            let excess = self.runs.len() - self.max_runs;
            self.runs.drain(..excess);
        }

        self.save()
    }

    /// Save history to disk.
    fn save(&self) -> crate::error::Result<()> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| TestxError::HistoryError {
            message: format!("Failed to create history dir: {e}"),
        })?;

        let history_file = self.data_dir.join("history.json");
        let content =
            serde_json::to_string_pretty(&self.runs).map_err(|e| TestxError::HistoryError {
                message: format!("Failed to serialize history: {e}"),
            })?;

        std::fs::write(&history_file, content).map_err(|e| TestxError::HistoryError {
            message: format!("Failed to write history: {e}"),
        })?;

        Ok(())
    }

    /// Get the number of recorded runs.
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Get all run records.
    pub fn runs(&self) -> &[RunRecord] {
        &self.runs
    }

    /// Get the most recent N runs.
    pub fn recent_runs(&self, n: usize) -> &[RunRecord] {
        let start = self.runs.len().saturating_sub(n);
        &self.runs[start..]
    }

    /// Get trend data for a specific test.
    pub fn get_trend(&self, test_name: &str, last_n: usize) -> Vec<TestTrend> {
        let runs = self.recent_runs(last_n);
        let mut trend = Vec::new();

        for run in runs {
            if let Some(test) = run.tests.iter().find(|t| t.name == test_name) {
                trend.push(TestTrend {
                    timestamp: run.timestamp.clone(),
                    status: test.status.clone(),
                    duration_ms: test.duration_ms,
                });
            }
        }

        trend
    }

    /// Get flaky tests (tests that alternate between pass and fail).
    pub fn get_flaky_tests(&self, min_runs: usize, max_pass_rate: f64) -> Vec<FlakyTest> {
        let recent = self.recent_runs(50);
        let mut test_history: HashMap<String, Vec<bool>> = HashMap::new();

        for run in recent {
            for test in &run.tests {
                let passed = test.status == "passed";
                test_history
                    .entry(test.name.clone())
                    .or_default()
                    .push(passed);
            }
        }

        let mut flaky = Vec::new();
        for (name, results) in &test_history {
            if results.len() < min_runs {
                continue;
            }

            let passes = results.iter().filter(|&&r| r).count();
            let pass_rate = passes as f64 / results.len() as f64;

            // A test is flaky if it has a pass rate between max_pass_rate and (1 - max_pass_rate)
            if pass_rate > 0.0 && pass_rate < max_pass_rate {
                let recent: String = results
                    .iter()
                    .rev()
                    .take(10)
                    .map(|&r| if r { 'P' } else { 'F' })
                    .collect();

                flaky.push(FlakyTest {
                    name: name.clone(),
                    pass_rate,
                    total_runs: results.len(),
                    failures: results.len() - passes,
                    recent_pattern: recent,
                });
            }
        }

        flaky.sort_by(|a, b| {
            a.pass_rate
                .partial_cmp(&b.pass_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        flaky
    }

    /// Get tests that are getting slower over time.
    pub fn get_slowest_trending(&self, last_n: usize, min_runs: usize) -> Vec<SlowTest> {
        let recent = self.recent_runs(last_n);
        let mut test_durations: HashMap<String, Vec<u64>> = HashMap::new();

        for run in recent {
            for test in &run.tests {
                if test.status == "passed" {
                    test_durations
                        .entry(test.name.clone())
                        .or_default()
                        .push(test.duration_ms);
                }
            }
        }

        let mut slow_tests = Vec::new();
        for (name, durations) in &test_durations {
            if durations.len() < min_runs {
                continue;
            }

            let avg: u64 = durations.iter().sum::<u64>() / durations.len() as u64;
            let latest = *durations.last().unwrap_or(&0);

            let change_pct = if avg > 0 {
                (latest as f64 - avg as f64) / avg as f64 * 100.0
            } else {
                0.0
            };

            let trend = if change_pct > 20.0 {
                DurationTrend::Slower
            } else if change_pct < -20.0 {
                DurationTrend::Faster
            } else {
                DurationTrend::Stable
            };

            slow_tests.push(SlowTest {
                name: name.clone(),
                avg_duration: Duration::from_millis(avg),
                latest_duration: Duration::from_millis(latest),
                trend,
                change_pct,
            });
        }

        slow_tests.sort_by(|a, b| {
            b.change_pct
                .partial_cmp(&a.change_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        slow_tests
    }

    /// Prune old entries, keeping only the most recent N.
    pub fn prune(&mut self, keep: usize) -> crate::error::Result<usize> {
        if self.runs.len() <= keep {
            return Ok(0);
        }
        let removed = self.runs.len() - keep;
        self.runs.drain(..removed);
        self.save()?;
        Ok(removed)
    }

    /// Get the overall pass rate over recent runs.
    pub fn pass_rate(&self, last_n: usize) -> f64 {
        let recent = self.recent_runs(last_n);
        if recent.is_empty() {
            return 0.0;
        }

        let total_passed: usize = recent.iter().map(|r| r.passed).sum();
        let total_tests: usize = recent.iter().map(|r| r.total).sum();

        if total_tests > 0 {
            total_passed as f64 / total_tests as f64 * 100.0
        } else {
            0.0
        }
    }

    /// Get the average duration over recent runs.
    pub fn avg_duration(&self, last_n: usize) -> Duration {
        let recent = self.recent_runs(last_n);
        if recent.is_empty() {
            return Duration::ZERO;
        }

        let total_ms: u64 = recent.iter().map(|r| r.duration_ms).sum();
        Duration::from_millis(total_ms / recent.len() as u64)
    }
}

impl RunRecord {
    /// Create a RunRecord from a TestRunResult.
    pub fn from_result(result: &TestRunResult) -> Self {
        let tests: Vec<TestRecord> = result
            .suites
            .iter()
            .flat_map(|suite| {
                suite.tests.iter().map(|test| {
                    let status = match test.status {
                        TestStatus::Passed => "passed",
                        TestStatus::Failed => "failed",
                        TestStatus::Skipped => "skipped",
                    };
                    TestRecord {
                        name: format!("{}::{}", suite.name, test.name),
                        status: status.to_string(),
                        duration_ms: test.duration.as_millis() as u64,
                        error: test.error.as_ref().map(|e| e.message.clone()),
                    }
                })
            })
            .collect();

        Self {
            timestamp: chrono_now(),
            total: result.total_tests(),
            passed: result.total_passed(),
            failed: result.total_failed(),
            skipped: result.total_skipped(),
            duration_ms: result.duration.as_millis() as u64,
            exit_code: result.raw_exit_code,
            tests,
        }
    }
}

/// Get current timestamp as ISO 8601 string (without chrono crate).
fn chrono_now() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple UTC timestamp calculation
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since Unix epoch to year/month/day (simplified)
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1)
}

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestError, TestSuite};

    fn make_test(name: &str, status: TestStatus, ms: u64) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(ms),
            error: None,
        }
    }

    fn make_failed_test(name: &str, ms: u64, msg: &str) -> TestCase {
        TestCase {
            name: name.into(),
            status: TestStatus::Failed,
            duration: Duration::from_millis(ms),
            error: Some(TestError {
                message: msg.into(),
                location: None,
            }),
        }
    }

    fn make_result(passed: usize, failed: usize, skipped: usize) -> TestRunResult {
        let mut tests = Vec::new();
        for i in 0..passed {
            tests.push(make_test(
                &format!("pass_{i}"),
                TestStatus::Passed,
                10 + i as u64,
            ));
        }
        for i in 0..failed {
            tests.push(make_failed_test(
                &format!("fail_{i}"),
                5,
                "assertion failed",
            ));
        }
        for i in 0..skipped {
            tests.push(make_test(&format!("skip_{i}"), TestStatus::Skipped, 0));
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

    #[test]
    fn new_in_memory() {
        let history = TestHistory::new_in_memory();
        assert_eq!(history.run_count(), 0);
    }

    #[test]
    fn record_run() {
        let mut history = TestHistory::new_in_memory();
        // Don't save to disk in tests
        history
            .runs
            .push(RunRecord::from_result(&make_result(5, 1, 0)));
        assert_eq!(history.run_count(), 1);
    }

    #[test]
    fn run_record_from_result() {
        let result = make_result(3, 1, 1);
        let record = RunRecord::from_result(&result);
        assert_eq!(record.total, 5);
        assert_eq!(record.passed, 3);
        assert_eq!(record.failed, 1);
        assert_eq!(record.skipped, 1);
        assert_eq!(record.tests.len(), 5);
    }

    #[test]
    fn run_record_test_names() {
        let result = make_result(2, 0, 0);
        let record = RunRecord::from_result(&result);
        assert_eq!(record.tests[0].name, "suite::pass_0");
        assert_eq!(record.tests[1].name, "suite::pass_1");
    }

    #[test]
    fn run_record_error_captured() {
        let result = make_result(0, 1, 0);
        let record = RunRecord::from_result(&result);
        assert_eq!(record.tests[0].error.as_deref(), Some("assertion failed"));
    }

    #[test]
    fn recent_runs() {
        let mut history = TestHistory::new_in_memory();
        for _ in 0..10 {
            history
                .runs
                .push(RunRecord::from_result(&make_result(5, 0, 0)));
        }
        assert_eq!(history.recent_runs(3).len(), 3);
        assert_eq!(history.recent_runs(20).len(), 10);
    }

    #[test]
    fn get_trend() {
        let mut history = TestHistory::new_in_memory();
        for i in 0..5 {
            let mut record = RunRecord::from_result(&make_result(3, 0, 0));
            record.tests[0].duration_ms = 10 + i * 5;
            history.runs.push(record);
        }

        let trend = history.get_trend("suite::pass_0", 10);
        assert_eq!(trend.len(), 5);
        assert_eq!(trend[0].duration_ms, 10);
        assert_eq!(trend[4].duration_ms, 30);
    }

    #[test]
    fn get_flaky_tests() {
        let mut history = TestHistory::new_in_memory();

        // Create alternating pass/fail for the same test name
        for i in 0..10 {
            let status = if i % 2 == 0 {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            let result = TestRunResult {
                suites: vec![TestSuite {
                    name: "suite".into(),
                    tests: vec![TestCase {
                        name: "flaky_test".into(),
                        status,
                        duration: Duration::from_millis(10),
                        error: None,
                    }],
                }],
                duration: Duration::from_millis(50),
                raw_exit_code: 0,
            };
            history.runs.push(RunRecord::from_result(&result));
        }

        let flaky = history.get_flaky_tests(5, 0.95);
        // The test should appear as flaky (50% pass rate)
        assert!(!flaky.is_empty());
    }

    #[test]
    fn get_flaky_no_flaky() {
        let mut history = TestHistory::new_in_memory();
        for _ in 0..10 {
            history
                .runs
                .push(RunRecord::from_result(&make_result(5, 0, 0)));
        }

        let flaky = history.get_flaky_tests(5, 0.95);
        assert!(flaky.is_empty());
    }

    #[test]
    fn get_slowest_trending() {
        let mut history = TestHistory::new_in_memory();

        for i in 0..10 {
            let mut record = RunRecord::from_result(&make_result(2, 0, 0));
            // Make test progressively slower
            record.tests[0].duration_ms = 100 + i * 50;
            record.tests[1].duration_ms = 50; // stable
            history.runs.push(record);
        }

        let slow = history.get_slowest_trending(10, 5);
        assert!(!slow.is_empty());
        // First test should be trending slower
        let first = slow.iter().find(|s| s.name.contains("pass_0"));
        assert!(first.is_some());
    }

    #[test]
    fn pass_rate_all_pass() {
        let mut history = TestHistory::new_in_memory();
        for _ in 0..5 {
            history
                .runs
                .push(RunRecord::from_result(&make_result(10, 0, 0)));
        }
        assert_eq!(history.pass_rate(10), 100.0);
    }

    #[test]
    fn pass_rate_mixed() {
        let mut history = TestHistory::new_in_memory();
        history
            .runs
            .push(RunRecord::from_result(&make_result(8, 2, 0)));
        assert!((history.pass_rate(10) - 80.0).abs() < 0.1);
    }

    #[test]
    fn pass_rate_empty() {
        let history = TestHistory::new_in_memory();
        assert_eq!(history.pass_rate(10), 0.0);
    }

    #[test]
    fn avg_duration() {
        let mut history = TestHistory::new_in_memory();
        for _ in 0..4 {
            let mut record = RunRecord::from_result(&make_result(1, 0, 0));
            record.duration_ms = 100;
            history.runs.push(record);
        }
        assert_eq!(history.avg_duration(10), Duration::from_millis(100));
    }

    #[test]
    fn prune_runs() {
        let mut history = TestHistory::new_in_memory();
        for _ in 0..20 {
            history
                .runs
                .push(RunRecord::from_result(&make_result(1, 0, 0)));
        }
        // Can't save to real path, just test the pruning logic
        let before = history.run_count();
        history.runs.drain(..10);
        assert_eq!(history.run_count(), before - 10);
    }

    #[test]
    fn days_to_date_epoch() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_date_known() {
        // 2024-01-01 is 19723 days from epoch
        let (y, m, d) = days_to_date(19723);
        assert_eq!(y, 2024);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }

    #[test]
    fn leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn chrono_now_format() {
        let ts = chrono_now();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
    }

    #[test]
    fn duration_trend_variants() {
        assert_eq!(DurationTrend::Faster, DurationTrend::Faster);
        assert_ne!(DurationTrend::Faster, DurationTrend::Slower);
    }
}
