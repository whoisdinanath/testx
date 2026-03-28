use std::time::{Duration, Instant};

use crate::adapters::{TestRunResult, TestStatus};

/// Configuration for stress testing mode.
#[derive(Debug, Clone)]
pub struct StressConfig {
    /// Number of times to run the test suite.
    pub iterations: usize,
    /// Stop on first failure.
    pub fail_fast: bool,
    /// Maximum total duration for all iterations.
    pub max_duration: Option<Duration>,
}

impl StressConfig {
    pub fn new(iterations: usize) -> Self {
        Self {
            iterations,
            fail_fast: false,
            max_duration: None,
        }
    }

    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    pub fn with_max_duration(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }
}

impl Default for StressConfig {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Result of a single stress iteration.
#[derive(Debug, Clone)]
pub struct IterationResult {
    pub iteration: usize,
    pub result: TestRunResult,
    pub duration: Duration,
}

/// Aggregated stress test report.
#[derive(Debug, Clone)]
pub struct StressReport {
    pub iterations_completed: usize,
    pub iterations_requested: usize,
    pub total_duration: Duration,
    pub failures: Vec<IterationFailure>,
    pub flaky_tests: Vec<FlakyTestReport>,
    pub all_passed: bool,
    pub stopped_early: bool,
}

/// A specific failure in a stress test iteration.
#[derive(Debug, Clone)]
pub struct IterationFailure {
    pub iteration: usize,
    pub failed_tests: Vec<String>,
}

/// A test that was flaky across stress iterations.
#[derive(Debug, Clone)]
pub struct FlakyTestReport {
    pub name: String,
    pub suite: String,
    pub pass_count: usize,
    pub fail_count: usize,
    pub total_runs: usize,
    pub pass_rate: f64,
    pub durations: Vec<Duration>,
    pub avg_duration: Duration,
    pub max_duration: Duration,
    pub min_duration: Duration,
}

/// Accumulator that collects iteration results and produces a report.
pub struct StressAccumulator {
    config: StressConfig,
    iterations: Vec<IterationResult>,
    start_time: Instant,
}

impl StressAccumulator {
    pub fn new(config: StressConfig) -> Self {
        Self {
            config,
            iterations: Vec::new(),
            start_time: Instant::now(),
        }
    }

    /// Record one iteration's results. Returns true if we should continue.
    pub fn record(&mut self, result: TestRunResult, duration: Duration) -> bool {
        let iteration = self.iterations.len() + 1;
        let has_failures = result.total_failed() > 0;

        self.iterations.push(IterationResult {
            iteration,
            result,
            duration,
        });

        if self.config.fail_fast && has_failures {
            return false;
        }

        if let Some(max_dur) = self.config.max_duration
            && self.start_time.elapsed() >= max_dur
        {
            return false;
        }

        iteration < self.config.iterations
    }

    /// How many iterations have been completed.
    pub fn completed(&self) -> usize {
        self.iterations.len()
    }

    /// Total iterations requested.
    pub fn requested(&self) -> usize {
        self.config.iterations
    }

    /// Check if the max duration has been exceeded.
    pub fn is_time_exceeded(&self) -> bool {
        self.config
            .max_duration
            .is_some_and(|d| self.start_time.elapsed() >= d)
    }

    /// Build the final stress report.
    pub fn report(self) -> StressReport {
        let iterations_completed = self.iterations.len();
        let total_duration = self.start_time.elapsed();
        let stopped_early = iterations_completed < self.config.iterations;

        // Collect failures per iteration
        let failures: Vec<IterationFailure> = self
            .iterations
            .iter()
            .filter(|it| it.result.total_failed() > 0)
            .map(|it| {
                let failed_tests: Vec<String> = it
                    .result
                    .suites
                    .iter()
                    .flat_map(|s| {
                        s.tests
                            .iter()
                            .filter(|t| t.status == TestStatus::Failed)
                            .map(move |t| format!("{}::{}", s.name, t.name))
                    })
                    .collect();

                IterationFailure {
                    iteration: it.iteration,
                    failed_tests,
                }
            })
            .collect();

        // Analyze flaky tests: tests that both passed and failed across iterations
        let flaky_tests = analyze_flaky_tests(&self.iterations);

        let all_passed = failures.is_empty();

        StressReport {
            iterations_completed,
            iterations_requested: self.config.iterations,
            total_duration,
            failures,
            flaky_tests,
            all_passed,
            stopped_early,
        }
    }
}

/// Analyze test results across iterations to find flaky tests.
fn analyze_flaky_tests(iterations: &[IterationResult]) -> Vec<FlakyTestReport> {
    use std::collections::HashMap;

    // Track per-test status across iterations: (suite, test) -> vec of (status, duration)
    let mut test_history: HashMap<(String, String), Vec<(TestStatus, Duration)>> = HashMap::new();

    for iteration in iterations {
        for suite in &iteration.result.suites {
            for test in &suite.tests {
                test_history
                    .entry((suite.name.clone(), test.name.clone()))
                    .or_default()
                    .push((test.status.clone(), test.duration));
            }
        }
    }

    let mut flaky_tests: Vec<FlakyTestReport> = test_history
        .into_iter()
        .filter_map(|((suite, name), history)| {
            let pass_count = history
                .iter()
                .filter(|(s, _)| *s == TestStatus::Passed)
                .count();
            let fail_count = history
                .iter()
                .filter(|(s, _)| *s == TestStatus::Failed)
                .count();
            let total_runs = history.len();

            // A test is flaky if it both passed and failed
            if pass_count > 0 && fail_count > 0 {
                let durations: Vec<Duration> = history.iter().map(|(_, d)| *d).collect();
                let total_dur: Duration = durations.iter().sum();
                let avg_duration = total_dur / total_runs as u32;
                let max_duration = durations.iter().copied().max().unwrap_or_default();
                let min_duration = durations.iter().copied().min().unwrap_or_default();

                Some(FlakyTestReport {
                    name,
                    suite,
                    pass_count,
                    fail_count,
                    total_runs,
                    pass_rate: pass_count as f64 / total_runs as f64 * 100.0,
                    durations,
                    avg_duration,
                    max_duration,
                    min_duration,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by pass rate (lowest = most flaky)
    flaky_tests.sort_by(|a, b| {
        a.pass_rate
            .partial_cmp(&b.pass_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    flaky_tests
}

/// Format a stress report for display.
pub fn format_stress_report(report: &StressReport) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Stress Test Report: {}/{} iterations in {:.2}s",
        report.iterations_completed,
        report.iterations_requested,
        report.total_duration.as_secs_f64(),
    ));

    if report.stopped_early {
        lines.push("  (stopped early)".to_string());
    }

    lines.push(String::new());

    if report.all_passed {
        lines.push(format!(
            "  All {} iterations passed — no flaky tests detected!",
            report.iterations_completed
        ));
    } else {
        lines.push(format!(
            "  {} iteration(s) had failures",
            report.failures.len()
        ));

        for failure in &report.failures {
            lines.push(format!("  Iteration {}:", failure.iteration));
            for test in &failure.failed_tests {
                lines.push(format!("    - {}", test));
            }
        }
    }

    if !report.flaky_tests.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "  Flaky tests detected ({}):",
            report.flaky_tests.len()
        ));
        for flaky in &report.flaky_tests {
            lines.push(format!(
                "    {} ({}/{} passed, {:.1}% pass rate, avg {:.1}ms)",
                flaky.name,
                flaky.pass_count,
                flaky.total_runs,
                flaky.pass_rate,
                flaky.avg_duration.as_secs_f64() * 1000.0,
            ));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestError, TestSuite};

    fn make_passing_result(num_tests: usize) -> TestRunResult {
        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".to_string(),
                tests: (0..num_tests)
                    .map(|i| TestCase {
                        name: format!("test_{}", i),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(10),
                        error: None,
                    })
                    .collect(),
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: 0,
        }
    }

    fn make_mixed_result(pass: usize, fail: usize) -> TestRunResult {
        let mut tests: Vec<TestCase> = (0..pass)
            .map(|i| TestCase {
                name: format!("pass_{}", i),
                status: TestStatus::Passed,
                duration: Duration::from_millis(10),
                error: None,
            })
            .collect();

        for i in 0..fail {
            tests.push(TestCase {
                name: format!("fail_{}", i),
                status: TestStatus::Failed,
                duration: Duration::from_millis(10),
                error: Some(TestError {
                    message: "assertion failed".to_string(),
                    location: None,
                }),
            });
        }

        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".to_string(),
                tests,
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: 1,
        }
    }

    #[test]
    fn stress_config_defaults() {
        let cfg = StressConfig::default();
        assert_eq!(cfg.iterations, 10);
        assert!(!cfg.fail_fast);
        assert!(cfg.max_duration.is_none());
    }

    #[test]
    fn stress_config_builder() {
        let cfg = StressConfig::new(100)
            .with_fail_fast(true)
            .with_max_duration(Duration::from_secs(60));

        assert_eq!(cfg.iterations, 100);
        assert!(cfg.fail_fast);
        assert_eq!(cfg.max_duration, Some(Duration::from_secs(60)));
    }

    #[test]
    fn accumulator_all_passing() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        assert!(acc.record(make_passing_result(5), Duration::from_millis(100)));
        assert!(acc.record(make_passing_result(5), Duration::from_millis(100)));
        assert!(!acc.record(make_passing_result(5), Duration::from_millis(100)));

        let report = acc.report();
        assert!(report.all_passed);
        assert_eq!(report.iterations_completed, 3);
        assert_eq!(report.iterations_requested, 3);
        assert!(report.failures.is_empty());
        assert!(report.flaky_tests.is_empty());
        assert!(!report.stopped_early);
    }

    #[test]
    fn accumulator_fail_fast() {
        let cfg = StressConfig::new(10).with_fail_fast(true);
        let mut acc = StressAccumulator::new(cfg);

        assert!(acc.record(make_passing_result(5), Duration::from_millis(100)));
        // Second iteration fails — should stop
        assert!(!acc.record(make_mixed_result(3, 2), Duration::from_millis(100)));

        let report = acc.report();
        assert!(!report.all_passed);
        assert_eq!(report.iterations_completed, 2);
        assert!(report.stopped_early);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].iteration, 2);
    }

    #[test]
    fn accumulator_without_fail_fast() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        assert!(acc.record(make_passing_result(5), Duration::from_millis(100)));
        assert!(acc.record(make_mixed_result(3, 2), Duration::from_millis(100)));
        assert!(!acc.record(make_passing_result(5), Duration::from_millis(100)));

        let report = acc.report();
        assert!(!report.all_passed);
        assert_eq!(report.iterations_completed, 3);
        assert!(!report.stopped_early);
        assert_eq!(report.failures.len(), 1);
    }

    #[test]
    fn flaky_test_detection() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        // Iteration 1: test_0 passes
        acc.record(make_passing_result(3), Duration::from_millis(100));

        // Iteration 2: test_0 fails (make it flaky)
        let mut r2 = make_passing_result(3);
        r2.suites[0].tests[0].status = TestStatus::Failed;
        r2.suites[0].tests[0].error = Some(TestError {
            message: "flaky!".to_string(),
            location: None,
        });
        r2.raw_exit_code = 1;
        acc.record(r2, Duration::from_millis(100));

        // Iteration 3: test_0 passes again
        acc.record(make_passing_result(3), Duration::from_millis(100));

        let report = acc.report();
        assert_eq!(report.flaky_tests.len(), 1);
        assert_eq!(report.flaky_tests[0].name, "test_0");
        assert_eq!(report.flaky_tests[0].pass_count, 2);
        assert_eq!(report.flaky_tests[0].fail_count, 1);
        assert_eq!(report.flaky_tests[0].total_runs, 3);
    }

    #[test]
    fn consistently_failing_not_flaky() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        // All iterations have the same failure
        acc.record(make_mixed_result(3, 1), Duration::from_millis(100));
        acc.record(make_mixed_result(3, 1), Duration::from_millis(100));
        acc.record(make_mixed_result(3, 1), Duration::from_millis(100));

        let report = acc.report();
        // fail_0 always fails — not flaky, just broken
        assert!(report.flaky_tests.is_empty());
    }

    #[test]
    fn consistently_passing_not_flaky() {
        let cfg = StressConfig::new(5);
        let mut acc = StressAccumulator::new(cfg);

        for _ in 0..5 {
            acc.record(make_passing_result(3), Duration::from_millis(100));
        }

        let report = acc.report();
        assert!(report.flaky_tests.is_empty());
    }

    #[test]
    fn format_report_all_passing() {
        let report = StressReport {
            iterations_completed: 10,
            iterations_requested: 10,
            total_duration: Duration::from_secs(5),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
        };

        let output = format_stress_report(&report);
        assert!(output.contains("10/10 iterations"));
        assert!(output.contains("no flaky tests"));
    }

    #[test]
    fn format_report_with_failures() {
        let report = StressReport {
            iterations_completed: 5,
            iterations_requested: 10,
            total_duration: Duration::from_secs(3),
            failures: vec![IterationFailure {
                iteration: 3,
                failed_tests: vec!["suite::test_1".to_string()],
            }],
            flaky_tests: vec![FlakyTestReport {
                name: "test_1".to_string(),
                suite: "suite".to_string(),
                pass_count: 4,
                fail_count: 1,
                total_runs: 5,
                pass_rate: 80.0,
                durations: vec![Duration::from_millis(10); 5],
                avg_duration: Duration::from_millis(10),
                max_duration: Duration::from_millis(15),
                min_duration: Duration::from_millis(8),
            }],
            all_passed: false,
            stopped_early: true,
        };

        let output = format_stress_report(&report);
        assert!(output.contains("stopped early"));
        assert!(output.contains("Iteration 3"));
        assert!(output.contains("Flaky tests detected"));
        assert!(output.contains("80.0% pass rate"));
    }

    #[test]
    fn accumulator_completed_count() {
        let cfg = StressConfig::new(5);
        let mut acc = StressAccumulator::new(cfg);

        assert_eq!(acc.completed(), 0);
        assert_eq!(acc.requested(), 5);

        acc.record(make_passing_result(3), Duration::from_millis(100));
        assert_eq!(acc.completed(), 1);

        acc.record(make_passing_result(3), Duration::from_millis(100));
        assert_eq!(acc.completed(), 2);
    }

    #[test]
    fn flaky_test_duration_stats() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        // Three iterations with varying duration
        let mut r1 = make_passing_result(1);
        r1.suites[0].tests[0].duration = Duration::from_millis(10);
        acc.record(r1, Duration::from_millis(100));

        let mut r2 = make_passing_result(1);
        r2.suites[0].tests[0].status = TestStatus::Failed;
        r2.suites[0].tests[0].error = Some(TestError {
            message: "fail".to_string(),
            location: None,
        });
        r2.suites[0].tests[0].duration = Duration::from_millis(20);
        r2.raw_exit_code = 1;
        acc.record(r2, Duration::from_millis(100));

        let mut r3 = make_passing_result(1);
        r3.suites[0].tests[0].duration = Duration::from_millis(30);
        acc.record(r3, Duration::from_millis(100));

        let report = acc.report();
        assert_eq!(report.flaky_tests.len(), 1);
        let flaky = &report.flaky_tests[0];
        assert_eq!(flaky.min_duration, Duration::from_millis(10));
        assert_eq!(flaky.max_duration, Duration::from_millis(30));
        assert_eq!(flaky.avg_duration, Duration::from_millis(20));
    }

    #[test]
    fn multiple_flaky_tests_sorted_by_pass_rate() {
        let cfg = StressConfig::new(4);
        let mut acc = StressAccumulator::new(cfg);

        // Create results where test_a fails 3/4 times (25% pass rate)
        // and test_b fails 1/4 times (75% pass rate)
        for i in 0..4 {
            let result = TestRunResult {
                suites: vec![TestSuite {
                    name: "suite".to_string(),
                    tests: vec![
                        TestCase {
                            name: "test_a".to_string(),
                            status: if i == 0 {
                                TestStatus::Passed
                            } else {
                                TestStatus::Failed
                            },
                            duration: Duration::from_millis(10),
                            error: if i == 0 {
                                None
                            } else {
                                Some(TestError {
                                    message: "fail".into(),
                                    location: None,
                                })
                            },
                        },
                        TestCase {
                            name: "test_b".to_string(),
                            status: if i == 2 {
                                TestStatus::Failed
                            } else {
                                TestStatus::Passed
                            },
                            duration: Duration::from_millis(10),
                            error: if i == 2 {
                                Some(TestError {
                                    message: "fail".into(),
                                    location: None,
                                })
                            } else {
                                None
                            },
                        },
                    ],
                }],
                duration: Duration::from_millis(100),
                raw_exit_code: if i == 0 { 0 } else { 1 },
            };
            acc.record(result, Duration::from_millis(100));
        }

        let report = acc.report();
        assert_eq!(report.flaky_tests.len(), 2);

        // Should be sorted by pass rate (lowest first)
        assert_eq!(report.flaky_tests[0].name, "test_a");
        assert_eq!(report.flaky_tests[1].name, "test_b");
        assert!(report.flaky_tests[0].pass_rate < report.flaky_tests[1].pass_rate);
    }
}
