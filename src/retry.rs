use std::time::Duration;

use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};

/// Strategy for computing delay between retries.
#[derive(Debug, Clone)]
#[derive(Default)]
pub enum BackoffStrategy {
    /// No delay between retries
    #[default]
    None,
    /// Fixed delay between retries
    Fixed(Duration),
    /// Linear backoff: delay * attempt_number
    Linear(Duration),
    /// Exponential backoff: delay * 2^attempt_number, capped at max
    Exponential { base: Duration, max: Duration },
}

impl BackoffStrategy {
    /// Compute the delay for given attempt number (0-indexed).
    pub fn delay_for(&self, attempt: u32) -> Duration {
        match self {
            BackoffStrategy::None => Duration::ZERO,
            BackoffStrategy::Fixed(d) => *d,
            BackoffStrategy::Linear(base) => *base * attempt,
            BackoffStrategy::Exponential { base, max } => {
                let multiplier = 2u64.saturating_pow(attempt);
                let delay = base.saturating_mul(multiplier as u32);
                if delay > *max { *max } else { delay }
            }
        }
    }
}


/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries (0 = no retries)
    pub max_retries: u32,
    /// Backoff strategy between retries
    pub backoff: BackoffStrategy,
    /// Whether to stop retrying a test after it passes once
    pub stop_on_pass: bool,
    /// Whether to retry only failed tests (vs. retry entire suite)
    pub retry_failed_only: bool,
}

impl RetryConfig {
    /// Create a new retry config with the given max retries.
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            backoff: BackoffStrategy::None,
            stop_on_pass: true,
            retry_failed_only: true,
        }
    }

    /// Set the backoff strategy.
    pub fn with_backoff(mut self, backoff: BackoffStrategy) -> Self {
        self.backoff = backoff;
        self
    }

    /// Set whether to stop on first pass.
    pub fn with_stop_on_pass(mut self, stop: bool) -> Self {
        self.stop_on_pass = stop;
        self
    }

    /// Set whether to retry failed tests only.
    pub fn with_retry_failed_only(mut self, failed_only: bool) -> Self {
        self.retry_failed_only = failed_only;
        self
    }

    /// Returns true if retries are enabled.
    pub fn is_enabled(&self) -> bool {
        self.max_retries > 0
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Result of a single retry attempt.
#[derive(Debug, Clone)]
pub struct RetryAttempt {
    /// The attempt number (1-indexed, where 1 = first retry)
    pub attempt: u32,
    /// Result of this attempt
    pub result: TestRunResult,
    /// How long this attempt took
    pub duration: Duration,
}

/// Aggregated result of all retry attempts for a test run.
#[derive(Debug, Clone)]
pub struct RetryResult {
    /// Original (first) run result
    pub original: TestRunResult,
    /// Results of each retry attempt
    pub attempts: Vec<RetryAttempt>,
    /// Final merged result after all retries
    pub final_result: TestRunResult,
    /// Total number of attempts (including original)
    pub total_attempts: u32,
}

impl RetryResult {
    /// How many tests were fixed by retries.
    pub fn tests_fixed(&self) -> usize {
        let original_failed = self.original.total_failed();
        let final_failed = self.final_result.total_failed();
        original_failed.saturating_sub(final_failed)
    }

    /// Whether all tests pass after retries.
    pub fn all_passed(&self) -> bool {
        self.final_result.total_failed() == 0
    }

    /// Whether retries changed the outcome.
    pub fn had_effect(&self) -> bool {
        self.original.total_failed() != self.final_result.total_failed()
    }
}

/// Extract the names of failed tests from a result.
pub fn extract_failed_tests(result: &TestRunResult) -> Vec<FailedTestInfo> {
    let mut failed = Vec::new();
    for suite in &result.suites {
        for test in &suite.tests {
            if test.status == TestStatus::Failed {
                failed.push(FailedTestInfo {
                    suite_name: suite.name.clone(),
                    test_name: test.name.clone(),
                    error_message: test.error.as_ref().map(|e| e.message.clone()),
                });
            }
        }
    }
    failed
}

/// Information about a failed test.
#[derive(Debug, Clone)]
pub struct FailedTestInfo {
    /// Name of the suite containing this test
    pub suite_name: String,
    /// Name of the test
    pub test_name: String,
    /// Error message if available
    pub error_message: Option<String>,
}

impl FailedTestInfo {
    /// Fully qualified test name (suite::test).
    pub fn full_name(&self) -> String {
        format!("{}::{}", self.suite_name, self.test_name)
    }
}

/// Merge an original result with a retry result.
/// Tests that passed in the retry override their failed status in the original.
pub fn merge_retry_result(original: &TestRunResult, retry: &TestRunResult) -> TestRunResult {
    let mut suites = Vec::new();

    for orig_suite in &original.suites {
        // Find matching suite in retry result
        let retry_suite = retry
            .suites
            .iter()
            .find(|s| s.name == orig_suite.name);

        let tests: Vec<TestCase> = orig_suite
            .tests
            .iter()
            .map(|orig_test| {
                if orig_test.status != TestStatus::Failed {
                    return orig_test.clone();
                }

                // Look for this test in the retry result
                if let Some(rs) = retry_suite
                    && let Some(retry_test) = rs.tests.iter().find(|t| t.name == orig_test.name)
                        && retry_test.status == TestStatus::Passed {
                            // Test was fixed by retry
                            return retry_test.clone();
                        }

                orig_test.clone()
            })
            .collect();

        suites.push(TestSuite {
            name: orig_suite.name.clone(),
            tests,
        });
    }

    let exit_code = if suites.iter().all(|s| s.failed() == 0) {
        0
    } else {
        original.raw_exit_code
    };

    TestRunResult {
        suites,
        duration: original.duration + retry.duration,
        raw_exit_code: exit_code,
    }
}

/// Merge results from multiple retry attempts progressively.
pub fn merge_all_retries(original: &TestRunResult, attempts: &[RetryAttempt]) -> TestRunResult {
    let mut merged = original.clone();
    for attempt in attempts {
        merged = merge_retry_result(&merged, &attempt.result);
    }
    merged
}

/// Build a RetryResult from an original run and retry attempts.
pub fn build_retry_result(original: TestRunResult, attempts: Vec<RetryAttempt>) -> RetryResult {
    let total_attempts = 1 + attempts.len() as u32;
    let final_result = merge_all_retries(&original, &attempts);

    RetryResult {
        original,
        attempts,
        final_result,
        total_attempts,
    }
}

/// Determine which tests still need retrying after an attempt.
pub fn tests_still_failing(
    current: &TestRunResult,
    failed_names: &[FailedTestInfo],
) -> Vec<FailedTestInfo> {
    let mut still_failing = Vec::new();

    for info in failed_names {
        let still_failed = current.suites.iter().any(|suite| {
            suite.name == info.suite_name
                && suite
                    .tests
                    .iter()
                    .any(|t| t.name == info.test_name && t.status == TestStatus::Failed)
        });

        if still_failed {
            still_failing.push(info.clone());
        }
    }

    still_failing
}

/// Create a filter string from failed test names for re-running.
pub fn failed_tests_as_filter(failed: &[FailedTestInfo]) -> String {
    failed
        .iter()
        .map(|f| f.test_name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Statistics about a retry session.
#[derive(Debug, Clone)]
pub struct RetryStats {
    /// Total retry attempts made
    pub total_retries: u32,
    /// Total tests retried
    pub tests_retried: usize,
    /// Tests fixed by retries
    pub tests_fixed: usize,
    /// Tests still failing after all retries
    pub tests_still_failing: usize,
    /// Total time spent retrying
    pub total_retry_time: Duration,
}

/// Compute retry statistics from a RetryResult.
pub fn compute_retry_stats(result: &RetryResult) -> RetryStats {
    let original_failed = result.original.total_failed();
    let final_failed = result.final_result.total_failed();
    let total_retry_time: Duration = result.attempts.iter().map(|a| a.duration).sum();

    RetryStats {
        total_retries: result.attempts.len() as u32,
        tests_retried: original_failed,
        tests_fixed: original_failed.saturating_sub(final_failed),
        tests_still_failing: final_failed,
        total_retry_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test(name: &str, status: TestStatus) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(10),
            error: None,
        }
    }

    fn make_suite(name: &str, tests: Vec<TestCase>) -> TestSuite {
        TestSuite {
            name: name.into(),
            tests,
        }
    }

    fn make_result(suites: Vec<TestSuite>) -> TestRunResult {
        TestRunResult {
            suites,
            duration: Duration::from_millis(100),
            raw_exit_code: 1,
        }
    }

    // ─── BackoffStrategy Tests ──────────────────────────────────────────

    #[test]
    fn backoff_none() {
        let b = BackoffStrategy::None;
        assert_eq!(b.delay_for(0), Duration::ZERO);
        assert_eq!(b.delay_for(5), Duration::ZERO);
    }

    #[test]
    fn backoff_fixed() {
        let b = BackoffStrategy::Fixed(Duration::from_millis(500));
        assert_eq!(b.delay_for(0), Duration::from_millis(500));
        assert_eq!(b.delay_for(3), Duration::from_millis(500));
    }

    #[test]
    fn backoff_linear() {
        let b = BackoffStrategy::Linear(Duration::from_millis(100));
        assert_eq!(b.delay_for(0), Duration::ZERO);
        assert_eq!(b.delay_for(1), Duration::from_millis(100));
        assert_eq!(b.delay_for(3), Duration::from_millis(300));
    }

    #[test]
    fn backoff_exponential() {
        let b = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            max: Duration::from_secs(5),
        };
        assert_eq!(b.delay_for(0), Duration::from_millis(100)); // 100 * 2^0
        assert_eq!(b.delay_for(1), Duration::from_millis(200)); // 100 * 2^1
        assert_eq!(b.delay_for(2), Duration::from_millis(400)); // 100 * 2^2
        assert_eq!(b.delay_for(3), Duration::from_millis(800)); // 100 * 2^3
    }

    #[test]
    fn backoff_exponential_cap() {
        let b = BackoffStrategy::Exponential {
            base: Duration::from_secs(1),
            max: Duration::from_secs(10),
        };
        assert_eq!(b.delay_for(10), Duration::from_secs(10)); // capped at max
    }

    // ─── RetryConfig Tests ──────────────────────────────────────────────

    #[test]
    fn retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 0);
        assert!(!config.is_enabled());
    }

    #[test]
    fn retry_config_enabled() {
        let config = RetryConfig::new(3);
        assert!(config.is_enabled());
        assert_eq!(config.max_retries, 3);
        assert!(config.stop_on_pass);
        assert!(config.retry_failed_only);
    }

    #[test]
    fn retry_config_builder() {
        let config = RetryConfig::new(2)
            .with_backoff(BackoffStrategy::Fixed(Duration::from_secs(1)))
            .with_stop_on_pass(false)
            .with_retry_failed_only(false);

        assert_eq!(config.max_retries, 2);
        assert!(!config.stop_on_pass);
        assert!(!config.retry_failed_only);
    }

    // ─── Extract Failed Tests ───────────────────────────────────────────

    #[test]
    fn extract_failed_test_info() {
        let result = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_add", TestStatus::Passed),
                make_test("test_div", TestStatus::Failed),
                make_test("test_mul", TestStatus::Passed),
            ],
        )]);

        let failed = extract_failed_tests(&result);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].test_name, "test_div");
        assert_eq!(failed[0].suite_name, "unit");
        assert_eq!(failed[0].full_name(), "unit::test_div");
    }

    #[test]
    fn extract_failed_multiple_suites() {
        let result = make_result(vec![
            make_suite(
                "math",
                vec![
                    make_test("test_add", TestStatus::Failed),
                    make_test("test_sub", TestStatus::Passed),
                ],
            ),
            make_suite(
                "strings",
                vec![
                    make_test("test_concat", TestStatus::Failed),
                ],
            ),
        ]);

        let failed = extract_failed_tests(&result);
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0].test_name, "test_add");
        assert_eq!(failed[1].test_name, "test_concat");
    }

    #[test]
    fn extract_failed_none() {
        let result = make_result(vec![make_suite(
            "unit",
            vec![make_test("test", TestStatus::Passed)],
        )]);

        let failed = extract_failed_tests(&result);
        assert!(failed.is_empty());
    }

    // ─── Merge Tests ────────────────────────────────────────────────────

    #[test]
    fn merge_retry_fixes_test() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Passed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let retry = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_b", TestStatus::Passed)],
        )]);

        let merged = merge_retry_result(&original, &retry);
        assert_eq!(merged.total_passed(), 2);
        assert_eq!(merged.total_failed(), 0);
        assert_eq!(merged.raw_exit_code, 0); // changed to 0 because all pass
    }

    #[test]
    fn merge_retry_still_fails() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_b", TestStatus::Failed)],
        )]);

        let retry = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_b", TestStatus::Failed)],
        )]);

        let merged = merge_retry_result(&original, &retry);
        assert_eq!(merged.total_failed(), 1);
        assert_eq!(merged.raw_exit_code, 1);
    }

    #[test]
    fn merge_retry_partial_fix() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Failed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let retry = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Passed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let merged = merge_retry_result(&original, &retry);
        assert_eq!(merged.total_passed(), 1);
        assert_eq!(merged.total_failed(), 1);
    }

    #[test]
    fn merge_no_matching_suite() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_a", TestStatus::Failed)],
        )]);

        let retry = make_result(vec![make_suite(
            "other",
            vec![make_test("test_a", TestStatus::Passed)],
        )]);

        let merged = merge_retry_result(&original, &retry);
        // No match, original status preserved
        assert_eq!(merged.total_failed(), 1);
    }

    // ─── Merge All Retries ──────────────────────────────────────────────

    #[test]
    fn merge_all_progressive() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Failed),
                make_test("test_b", TestStatus::Failed),
                make_test("test_c", TestStatus::Failed),
            ],
        )]);

        let attempt1 = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_a", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(50),
        };

        let attempt2 = RetryAttempt {
            attempt: 2,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_b", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(50),
        };

        let merged = merge_all_retries(&original, &[attempt1, attempt2]);
        assert_eq!(merged.total_passed(), 2);
        assert_eq!(merged.total_failed(), 1); // test_c still fails
    }

    // ─── RetryResult Tests ─────────────────────────────────────────────

    #[test]
    fn retry_result_stats() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Failed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let attempt = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_a", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(50),
        };

        let retry_result = build_retry_result(original, vec![attempt]);

        assert_eq!(retry_result.total_attempts, 2);
        assert_eq!(retry_result.tests_fixed(), 1);
        assert!(!retry_result.all_passed());
        assert!(retry_result.had_effect());
    }

    #[test]
    fn retry_result_all_fixed() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_a", TestStatus::Failed)],
        )]);

        let attempt = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_a", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(50),
        };

        let retry_result = build_retry_result(original, vec![attempt]);
        assert!(retry_result.all_passed());
        assert!(retry_result.had_effect());
    }

    #[test]
    fn retry_result_no_effect() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_a", TestStatus::Failed)],
        )]);

        let attempt = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_a", TestStatus::Failed)],
            )]),
            duration: Duration::from_millis(50),
        };

        let retry_result = build_retry_result(original, vec![attempt]);
        assert!(!retry_result.had_effect());
    }

    // ─── Still Failing Tests ────────────────────────────────────────────

    #[test]
    fn tests_still_failing_some() {
        let current = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Passed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let failed = vec![
            FailedTestInfo {
                suite_name: "unit".into(),
                test_name: "test_a".into(),
                error_message: None,
            },
            FailedTestInfo {
                suite_name: "unit".into(),
                test_name: "test_b".into(),
                error_message: None,
            },
        ];

        let still = tests_still_failing(&current, &failed);
        assert_eq!(still.len(), 1);
        assert_eq!(still[0].test_name, "test_b");
    }

    #[test]
    fn tests_still_failing_none() {
        let current = make_result(vec![make_suite(
            "unit",
            vec![make_test("test_a", TestStatus::Passed)],
        )]);

        let failed = vec![FailedTestInfo {
            suite_name: "unit".into(),
            test_name: "test_a".into(),
            error_message: None,
        }];

        let still = tests_still_failing(&current, &failed);
        assert!(still.is_empty());
    }

    // ─── Filter String ──────────────────────────────────────────────────

    #[test]
    fn failed_as_filter_string() {
        let failed = vec![
            FailedTestInfo {
                suite_name: "unit".into(),
                test_name: "test_a".into(),
                error_message: None,
            },
            FailedTestInfo {
                suite_name: "unit".into(),
                test_name: "test_b".into(),
                error_message: None,
            },
        ];

        let filter = failed_tests_as_filter(&failed);
        assert_eq!(filter, "test_a,test_b");
    }

    #[test]
    fn failed_as_filter_empty() {
        let filter = failed_tests_as_filter(&[]);
        assert_eq!(filter, "");
    }

    // ─── Compute Stats ─────────────────────────────────────────────────

    #[test]
    fn compute_stats_basic() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("test_a", TestStatus::Failed),
                make_test("test_b", TestStatus::Failed),
            ],
        )]);

        let attempt = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("test_a", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(200),
        };

        let retry_result = build_retry_result(original, vec![attempt]);
        let stats = compute_retry_stats(&retry_result);

        assert_eq!(stats.total_retries, 1);
        assert_eq!(stats.tests_retried, 2);
        assert_eq!(stats.tests_fixed, 1);
        assert_eq!(stats.tests_still_failing, 1);
        assert_eq!(stats.total_retry_time, Duration::from_millis(200));
    }

    #[test]
    fn compute_stats_multiple_attempts() {
        let original = make_result(vec![make_suite(
            "unit",
            vec![
                make_test("a", TestStatus::Failed),
                make_test("b", TestStatus::Failed),
                make_test("c", TestStatus::Failed),
            ],
        )]);

        let a1 = RetryAttempt {
            attempt: 1,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("a", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(100),
        };

        let a2 = RetryAttempt {
            attempt: 2,
            result: make_result(vec![make_suite(
                "unit",
                vec![make_test("b", TestStatus::Passed)],
            )]),
            duration: Duration::from_millis(100),
        };

        let retry_result = build_retry_result(original, vec![a1, a2]);
        let stats = compute_retry_stats(&retry_result);

        assert_eq!(stats.total_retries, 2);
        assert_eq!(stats.tests_retried, 3);
        assert_eq!(stats.tests_fixed, 2);
        assert_eq!(stats.tests_still_failing, 1);
        assert_eq!(stats.total_retry_time, Duration::from_millis(200));
    }
}
