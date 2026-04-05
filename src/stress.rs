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
    /// Minimum pass rate threshold (0.0 - 1.0). Fails CI if any test is below this.
    pub threshold: Option<f64>,
    /// Number of parallel stress workers (0 = sequential).
    pub parallel_workers: usize,
}

impl StressConfig {
    pub fn new(iterations: usize) -> Self {
        Self {
            iterations,
            fail_fast: false,
            max_duration: None,
            threshold: None,
            parallel_workers: 0,
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

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = Some(threshold.clamp(0.0, 1.0));
        self
    }

    pub fn with_parallel_workers(mut self, workers: usize) -> Self {
        self.parallel_workers = workers;
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
    /// Whether the threshold check passed (None if no threshold set).
    pub threshold_passed: Option<bool>,
    /// The configured threshold, if any.
    pub threshold: Option<f64>,
    /// Per-iteration timing data for trend analysis.
    pub iteration_durations: Vec<Duration>,
    /// Statistical summary of iteration durations.
    pub timing_stats: Option<TimingStats>,
}

/// Statistical summary of timing data.
#[derive(Debug, Clone)]
pub struct TimingStats {
    pub mean_ms: f64,
    pub median_ms: f64,
    pub std_dev_ms: f64,
    /// Coefficient of variation (std_dev / mean). High CV = inconsistent timing.
    pub cv: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

/// Severity classification for flaky tests.
#[derive(Debug, Clone, PartialEq)]
pub enum FlakySeverity {
    /// Pass rate < 50% — almost always fails, likely a real bug
    Critical,
    /// Pass rate 50-80% — frequently flaky
    High,
    /// Pass rate 80-95% — occasionally flaky
    Medium,
    /// Pass rate > 95% — rarely flaky, possibly environment-dependent
    Low,
}

impl FlakySeverity {
    pub fn from_pass_rate(pass_rate: f64) -> Self {
        match pass_rate {
            r if r < 50.0 => FlakySeverity::Critical,
            r if r < 80.0 => FlakySeverity::High,
            r if r < 95.0 => FlakySeverity::Medium,
            _ => FlakySeverity::Low,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            FlakySeverity::Critical => "CRITICAL",
            FlakySeverity::High => "HIGH",
            FlakySeverity::Medium => "MEDIUM",
            FlakySeverity::Low => "LOW",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            FlakySeverity::Critical => "🔴",
            FlakySeverity::High => "🟠",
            FlakySeverity::Medium => "🟡",
            FlakySeverity::Low => "🟢",
        }
    }
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
    /// Severity classification.
    pub severity: FlakySeverity,
    /// Wilson score confidence interval lower bound (95% confidence).
    /// A more statistically rigorous measure of pass rate.
    pub wilson_lower: f64,
    /// Timing coefficient of variation for this specific test.
    pub timing_cv: f64,
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

        // Collect iteration durations
        let iteration_durations: Vec<Duration> =
            self.iterations.iter().map(|it| it.duration).collect();

        // Compute timing stats
        let timing_stats = compute_timing_stats(&iteration_durations);

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

        // Check threshold
        let threshold_passed = self
            .config
            .threshold
            .map(|threshold| flaky_tests.iter().all(|f| f.pass_rate / 100.0 >= threshold));

        StressReport {
            iterations_completed,
            iterations_requested: self.config.iterations,
            total_duration,
            failures,
            flaky_tests,
            all_passed,
            stopped_early,
            threshold_passed,
            threshold: self.config.threshold,
            iteration_durations,
            timing_stats,
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
                let pass_rate = pass_count as f64 / total_runs as f64 * 100.0;

                // Wilson score lower bound (95% confidence)
                let wilson_lower = wilson_score_lower(pass_count, total_runs, 1.96);

                // Timing coefficient of variation
                let timing_cv = compute_cv(&durations);

                let severity = FlakySeverity::from_pass_rate(pass_rate);

                Some(FlakyTestReport {
                    name,
                    suite,
                    pass_count,
                    fail_count,
                    total_runs,
                    pass_rate,
                    durations,
                    avg_duration,
                    max_duration,
                    min_duration,
                    severity,
                    wilson_lower,
                    timing_cv,
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

/// Wilson score confidence interval lower bound.
/// Gives a statistically meaningful lower bound on the true pass rate,
/// accounting for sample size. Better than raw pass_rate for small N.
fn wilson_score_lower(successes: usize, total: usize, z: f64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let spread = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((center - spread) / denominator).max(0.0)
}

/// Compute the coefficient of variation for a set of durations.
fn compute_cv(durations: &[Duration]) -> f64 {
    if durations.len() < 2 {
        return 0.0;
    }
    let values: Vec<f64> = durations.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    if mean == 0.0 {
        return 0.0;
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();
    std_dev / mean
}

/// Compute timing statistics for a set of durations.
fn compute_timing_stats(durations: &[Duration]) -> Option<TimingStats> {
    if durations.is_empty() {
        return None;
    }

    let mut ms_values: Vec<f64> = durations.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    let n = ms_values.len() as f64;

    let mean = ms_values.iter().sum::<f64>() / n;

    ms_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median = if ms_values.len().is_multiple_of(2) {
        let mid = ms_values.len() / 2;
        (ms_values[mid - 1] + ms_values[mid]) / 2.0
    } else {
        ms_values[ms_values.len() / 2]
    };

    let variance = if ms_values.len() > 1 {
        ms_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)
    } else {
        0.0
    };
    let std_dev = variance.sqrt();
    let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

    let p95_idx = ((ms_values.len() as f64 * 0.95).ceil() as usize)
        .min(ms_values.len())
        .saturating_sub(1);
    let p99_idx = ((ms_values.len() as f64 * 0.99).ceil() as usize)
        .min(ms_values.len())
        .saturating_sub(1);

    Some(TimingStats {
        mean_ms: mean,
        median_ms: median,
        std_dev_ms: std_dev,
        cv,
        p95_ms: ms_values[p95_idx],
        p99_ms: ms_values[p99_idx],
    })
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

    // Timing statistics
    if let Some(stats) = &report.timing_stats {
        lines.push(String::new());
        lines.push("  Timing Statistics:".to_string());
        lines.push(format!(
            "    Mean: {:.1}ms | Median: {:.1}ms | Std Dev: {:.1}ms",
            stats.mean_ms, stats.median_ms, stats.std_dev_ms
        ));
        lines.push(format!(
            "    P95: {:.1}ms | P99: {:.1}ms | CV: {:.2}",
            stats.p95_ms, stats.p99_ms, stats.cv
        ));
        if stats.cv > 0.3 {
            lines.push(
                "    ⚠ High timing variance detected — results may be environment-sensitive"
                    .to_string(),
            );
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
                "    {} [{}] {} ({}/{} passed, {:.1}% pass rate, wilson≥{:.1}%, avg {:.1}ms, cv={:.2})",
                flaky.severity.icon(),
                flaky.severity.label(),
                flaky.name,
                flaky.pass_count,
                flaky.total_runs,
                flaky.pass_rate,
                flaky.wilson_lower * 100.0,
                flaky.avg_duration.as_secs_f64() * 1000.0,
                flaky.timing_cv,
            ));
        }
    }

    // Threshold result
    if let (Some(threshold), Some(passed)) = (report.threshold, report.threshold_passed) {
        lines.push(String::new());
        if passed {
            lines.push(format!(
                "  ✅ Threshold check passed (minimum {:.0}% pass rate)",
                threshold * 100.0
            ));
        } else {
            lines.push(format!(
                "  ❌ Threshold check FAILED (minimum {:.0}% pass rate required)",
                threshold * 100.0
            ));
        }
    }

    lines.join("\n")
}

/// Produce a JSON representation of the stress report.
pub fn stress_report_json(report: &StressReport) -> serde_json::Value {
    let flaky: Vec<serde_json::Value> = report
        .flaky_tests
        .iter()
        .map(|f| {
            serde_json::json!({
                "name": f.name,
                "suite": f.suite,
                "pass_count": f.pass_count,
                "fail_count": f.fail_count,
                "total_runs": f.total_runs,
                "pass_rate": f.pass_rate,
                "severity": f.severity.label(),
                "wilson_lower": f.wilson_lower,
                "timing_cv": f.timing_cv,
                "avg_duration_ms": f.avg_duration.as_secs_f64() * 1000.0,
                "min_duration_ms": f.min_duration.as_secs_f64() * 1000.0,
                "max_duration_ms": f.max_duration.as_secs_f64() * 1000.0,
            })
        })
        .collect();

    let failures: Vec<serde_json::Value> = report
        .failures
        .iter()
        .map(|f| {
            serde_json::json!({
                "iteration": f.iteration,
                "failed_tests": f.failed_tests,
            })
        })
        .collect();

    let mut json = serde_json::json!({
        "iterations_completed": report.iterations_completed,
        "iterations_requested": report.iterations_requested,
        "total_duration_ms": report.total_duration.as_secs_f64() * 1000.0,
        "all_passed": report.all_passed,
        "stopped_early": report.stopped_early,
        "failures": failures,
        "flaky_tests": flaky,
    });

    if let Some(stats) = &report.timing_stats {
        json["timing_stats"] = serde_json::json!({
            "mean_ms": stats.mean_ms,
            "median_ms": stats.median_ms,
            "std_dev_ms": stats.std_dev_ms,
            "cv": stats.cv,
            "p95_ms": stats.p95_ms,
            "p99_ms": stats.p99_ms,
        });
    }

    if let Some(threshold) = report.threshold {
        json["threshold"] = serde_json::json!(threshold);
        json["threshold_passed"] = serde_json::json!(report.threshold_passed);
    }

    json
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
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![Duration::from_millis(500); 10],
            timing_stats: None,
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
                severity: FlakySeverity::Medium,
                wilson_lower: 0.449,
                timing_cv: 0.0,
            }],
            all_passed: false,
            stopped_early: true,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![Duration::from_millis(600); 5],
            timing_stats: None,
        };

        let output = format_stress_report(&report);
        assert!(output.contains("stopped early"));
        assert!(output.contains("Iteration 3"));
        assert!(output.contains("Flaky tests detected"));
        assert!(output.contains("80.0% pass rate"));
        assert!(output.contains("MEDIUM"));
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

    // ─── Wilson score tests ───

    #[test]
    fn wilson_score_zero_total() {
        assert_eq!(wilson_score_lower(0, 0, 1.96), 0.0);
    }

    #[test]
    fn wilson_score_all_pass() {
        let score = wilson_score_lower(10, 10, 1.96);
        assert!(
            score > 0.7,
            "all-pass wilson lower should be > 0.7, got {score}"
        );
        assert!(score < 1.0);
    }

    #[test]
    fn wilson_score_all_fail() {
        let score = wilson_score_lower(0, 10, 1.96);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn wilson_score_half() {
        let score = wilson_score_lower(5, 10, 1.96);
        assert!(
            score > 0.2,
            "50% pass rate wilson lower should be > 0.2, got {score}"
        );
        assert!(
            score < 0.5,
            "50% pass rate wilson lower should be < 0.5, got {score}"
        );
    }

    #[test]
    fn wilson_score_small_sample() {
        // With only 2 samples, uncertainty is high — lower bound should be much less than raw rate
        let score = wilson_score_lower(1, 2, 1.96);
        assert!(
            score < 0.5,
            "small sample should pull wilson lower bound down, got {score}"
        );
        assert!(score > 0.0);
    }

    // ─── Coefficient of variation tests ───

    #[test]
    fn cv_single_duration() {
        assert_eq!(compute_cv(&[Duration::from_millis(100)]), 0.0);
    }

    #[test]
    fn cv_identical_durations() {
        let d = vec![Duration::from_millis(100); 5];
        let cv = compute_cv(&d);
        assert!(
            cv.abs() < 1e-10,
            "identical durations should have cv ≈ 0, got {cv}"
        );
    }

    #[test]
    fn cv_varied_durations() {
        let d = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(50),
        ];
        let cv = compute_cv(&d);
        assert!(cv > 0.4, "varied durations should have cv > 0.4, got {cv}");
        assert!(cv < 0.7, "varied durations should have cv < 0.7, got {cv}");
    }

    #[test]
    fn cv_empty() {
        assert_eq!(compute_cv(&[]), 0.0);
    }

    // ─── Timing stats tests ───

    #[test]
    fn timing_stats_empty() {
        assert!(compute_timing_stats(&[]).is_none());
    }

    #[test]
    fn timing_stats_single() {
        let stats = compute_timing_stats(&[Duration::from_millis(100)]).unwrap();
        assert!((stats.mean_ms - 100.0).abs() < 0.1);
        assert!((stats.median_ms - 100.0).abs() < 0.1);
        assert!(stats.std_dev_ms.abs() < 0.1);
        assert!(stats.cv.abs() < 0.01);
    }

    #[test]
    fn timing_stats_even_count() {
        let d = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
        ];
        let stats = compute_timing_stats(&d).unwrap();
        assert!((stats.mean_ms - 25.0).abs() < 0.1);
        assert!((stats.median_ms - 25.0).abs() < 0.1); // (20+30)/2
        assert!(stats.p95_ms >= 30.0);
        assert!(stats.p99_ms >= 30.0);
    }

    #[test]
    fn timing_stats_odd_count() {
        let d = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
        ];
        let stats = compute_timing_stats(&d).unwrap();
        assert!((stats.median_ms - 20.0).abs() < 0.1);
    }

    #[test]
    fn timing_stats_percentiles() {
        // 100 values from 1 to 100
        let d: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        let stats = compute_timing_stats(&d).unwrap();
        assert!(
            stats.p95_ms >= 95.0,
            "p95 should be ≥ 95, got {}",
            stats.p95_ms
        );
        assert!(
            stats.p99_ms >= 99.0,
            "p99 should be ≥ 99, got {}",
            stats.p99_ms
        );
    }

    // ─── Severity classification tests ───

    #[test]
    fn severity_critical() {
        assert_eq!(FlakySeverity::from_pass_rate(0.0), FlakySeverity::Critical);
        assert_eq!(FlakySeverity::from_pass_rate(25.0), FlakySeverity::Critical);
        assert_eq!(FlakySeverity::from_pass_rate(49.9), FlakySeverity::Critical);
    }

    #[test]
    fn severity_high() {
        assert_eq!(FlakySeverity::from_pass_rate(50.0), FlakySeverity::High);
        assert_eq!(FlakySeverity::from_pass_rate(70.0), FlakySeverity::High);
        assert_eq!(FlakySeverity::from_pass_rate(79.9), FlakySeverity::High);
    }

    #[test]
    fn severity_medium() {
        assert_eq!(FlakySeverity::from_pass_rate(80.0), FlakySeverity::Medium);
        assert_eq!(FlakySeverity::from_pass_rate(90.0), FlakySeverity::Medium);
        assert_eq!(FlakySeverity::from_pass_rate(94.9), FlakySeverity::Medium);
    }

    #[test]
    fn severity_low() {
        assert_eq!(FlakySeverity::from_pass_rate(95.0), FlakySeverity::Low);
        assert_eq!(FlakySeverity::from_pass_rate(100.0), FlakySeverity::Low);
    }

    #[test]
    fn severity_labels_and_icons() {
        assert_eq!(FlakySeverity::Critical.label(), "CRITICAL");
        assert_eq!(FlakySeverity::High.label(), "HIGH");
        assert_eq!(FlakySeverity::Medium.label(), "MEDIUM");
        assert_eq!(FlakySeverity::Low.label(), "LOW");
        // Icons are emoji strings
        assert!(!FlakySeverity::Critical.icon().is_empty());
        assert!(!FlakySeverity::Low.icon().is_empty());
    }

    // ─── Threshold tests ───

    #[test]
    fn threshold_config_builder() {
        let cfg = StressConfig::new(10).with_threshold(0.8);
        assert_eq!(cfg.threshold, Some(0.8));
    }

    #[test]
    fn threshold_clamps_to_range() {
        let cfg = StressConfig::new(10).with_threshold(1.5);
        assert_eq!(cfg.threshold, Some(1.0));
        let cfg = StressConfig::new(10).with_threshold(-0.5);
        assert_eq!(cfg.threshold, Some(0.0));
    }

    #[test]
    fn threshold_passed_when_all_above() {
        let cfg = StressConfig::new(3).with_threshold(0.5);
        let mut acc = StressAccumulator::new(cfg);

        // Iteration 1: all pass
        acc.record(make_passing_result(3), Duration::from_millis(100));
        // Iteration 2: one test fails (makes it flaky at 50%)
        let mut r2 = make_passing_result(3);
        r2.suites[0].tests[0].status = TestStatus::Failed;
        r2.suites[0].tests[0].error = Some(TestError {
            message: "flaky".to_string(),
            location: None,
        });
        r2.raw_exit_code = 1;
        acc.record(r2, Duration::from_millis(100));
        // Iteration 3: all pass again (test_0 is 66% pass rate, above 50% threshold)
        acc.record(make_passing_result(3), Duration::from_millis(100));

        let report = acc.report();
        // test_0 has pass_rate = 66.7%, threshold = 50% → should pass
        assert_eq!(report.threshold_passed, Some(true));
    }

    #[test]
    fn threshold_fails_when_below() {
        let cfg = StressConfig::new(4).with_threshold(0.9);
        let mut acc = StressAccumulator::new(cfg);

        // Make test_0 flaky: pass 2/4 times = 50% pass rate, below 90% threshold
        acc.record(make_passing_result(3), Duration::from_millis(100));

        let mut r2 = make_passing_result(3);
        r2.suites[0].tests[0].status = TestStatus::Failed;
        r2.suites[0].tests[0].error = Some(TestError {
            message: "f".to_string(),
            location: None,
        });
        r2.raw_exit_code = 1;
        acc.record(r2, Duration::from_millis(100));

        let mut r3 = make_passing_result(3);
        r3.suites[0].tests[0].status = TestStatus::Failed;
        r3.suites[0].tests[0].error = Some(TestError {
            message: "f".to_string(),
            location: None,
        });
        r3.raw_exit_code = 1;
        acc.record(r3, Duration::from_millis(100));

        acc.record(make_passing_result(3), Duration::from_millis(100));

        let report = acc.report();
        // test_0 pass rate = 50%, threshold = 90% → fails
        assert_eq!(report.threshold_passed, Some(false));
    }

    #[test]
    fn no_threshold_returns_none() {
        let cfg = StressConfig::new(2);
        let mut acc = StressAccumulator::new(cfg);
        acc.record(make_passing_result(3), Duration::from_millis(100));
        acc.record(make_passing_result(3), Duration::from_millis(100));
        let report = acc.report();
        assert!(report.threshold_passed.is_none());
        assert!(report.threshold.is_none());
    }

    // ─── Timing stats in report ───

    #[test]
    fn report_contains_timing_stats() {
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);

        acc.record(make_passing_result(3), Duration::from_millis(100));
        acc.record(make_passing_result(3), Duration::from_millis(200));
        acc.record(make_passing_result(3), Duration::from_millis(300));

        let report = acc.report();
        assert!(report.timing_stats.is_some());
        let stats = report.timing_stats.unwrap();
        assert!((stats.mean_ms - 200.0).abs() < 1.0);
        assert_eq!(report.iteration_durations.len(), 3);
    }

    // ─── Stress report JSON ───

    #[test]
    fn stress_json_basic() {
        let report = StressReport {
            iterations_completed: 5,
            iterations_requested: 5,
            total_duration: Duration::from_secs(2),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![Duration::from_millis(400); 5],
            timing_stats: None,
        };

        let json = stress_report_json(&report);
        assert_eq!(json["iterations_completed"], 5);
        assert_eq!(json["all_passed"], true);
        assert!(json.get("threshold").is_none());
    }

    #[test]
    fn stress_json_with_threshold() {
        let report = StressReport {
            iterations_completed: 3,
            iterations_requested: 3,
            total_duration: Duration::from_secs(1),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: Some(true),
            threshold: Some(0.8),
            iteration_durations: vec![],
            timing_stats: None,
        };

        let json = stress_report_json(&report);
        assert_eq!(json["threshold"], 0.8);
        assert_eq!(json["threshold_passed"], true);
    }

    #[test]
    fn stress_json_with_flaky_tests() {
        let report = StressReport {
            iterations_completed: 3,
            iterations_requested: 3,
            total_duration: Duration::from_secs(1),
            failures: vec![IterationFailure {
                iteration: 2,
                failed_tests: vec!["test_a".to_string()],
            }],
            flaky_tests: vec![FlakyTestReport {
                name: "test_a".to_string(),
                suite: "suite".to_string(),
                pass_count: 2,
                fail_count: 1,
                total_runs: 3,
                pass_rate: 66.7,
                durations: vec![Duration::from_millis(10); 3],
                avg_duration: Duration::from_millis(10),
                max_duration: Duration::from_millis(12),
                min_duration: Duration::from_millis(8),
                severity: FlakySeverity::High,
                wilson_lower: 0.3,
                timing_cv: 0.1,
            }],
            all_passed: false,
            stopped_early: false,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![],
            timing_stats: None,
        };

        let json = stress_report_json(&report);
        assert_eq!(json["flaky_tests"][0]["name"], "test_a");
        assert_eq!(json["flaky_tests"][0]["severity"], "HIGH");
        assert_eq!(json["failures"][0]["iteration"], 2);
    }

    #[test]
    fn stress_json_with_timing_stats() {
        let report = StressReport {
            iterations_completed: 3,
            iterations_requested: 3,
            total_duration: Duration::from_secs(1),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![],
            timing_stats: Some(TimingStats {
                mean_ms: 100.0,
                median_ms: 95.0,
                std_dev_ms: 10.0,
                cv: 0.1,
                p95_ms: 120.0,
                p99_ms: 130.0,
            }),
        };

        let json = stress_report_json(&report);
        assert_eq!(json["timing_stats"]["mean_ms"], 100.0);
        assert_eq!(json["timing_stats"]["cv"], 0.1);
        assert_eq!(json["timing_stats"]["p95_ms"], 120.0);
    }

    // ─── Flaky severity in report ───

    #[test]
    fn flaky_tests_have_severity_and_wilson() {
        let cfg = StressConfig::new(4);
        let mut acc = StressAccumulator::new(cfg);

        // test_0: pass 1/4 = 25% → Critical
        for i in 0..4 {
            let mut r = make_passing_result(1);
            if i > 0 {
                r.suites[0].tests[0].status = TestStatus::Failed;
                r.suites[0].tests[0].error = Some(TestError {
                    message: "f".to_string(),
                    location: None,
                });
                r.raw_exit_code = 1;
            }
            acc.record(r, Duration::from_millis(100));
        }

        let report = acc.report();
        assert_eq!(report.flaky_tests.len(), 1);
        let flaky = &report.flaky_tests[0];
        assert_eq!(flaky.severity, FlakySeverity::Critical);
        assert!(flaky.wilson_lower >= 0.0);
        assert!(flaky.wilson_lower < 0.5);
    }

    // ─── Format report with timing stats ───

    #[test]
    fn format_report_shows_timing_stats() {
        let report = StressReport {
            iterations_completed: 10,
            iterations_requested: 10,
            total_duration: Duration::from_secs(5),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![],
            timing_stats: Some(TimingStats {
                mean_ms: 500.0,
                median_ms: 480.0,
                std_dev_ms: 50.0,
                cv: 0.1,
                p95_ms: 600.0,
                p99_ms: 650.0,
            }),
        };

        let output = format_stress_report(&report);
        assert!(output.contains("Timing Statistics"));
        assert!(output.contains("Mean: 500.0ms"));
        assert!(output.contains("P95: 600.0ms"));
    }

    #[test]
    fn format_report_shows_threshold_pass() {
        let report = StressReport {
            iterations_completed: 5,
            iterations_requested: 5,
            total_duration: Duration::from_secs(2),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: Some(true),
            threshold: Some(0.9),
            iteration_durations: vec![],
            timing_stats: None,
        };

        let output = format_stress_report(&report);
        assert!(output.contains("Threshold check passed"));
    }

    #[test]
    fn format_report_shows_threshold_fail() {
        let report = StressReport {
            iterations_completed: 5,
            iterations_requested: 5,
            total_duration: Duration::from_secs(2),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: Some(false),
            threshold: Some(0.95),
            iteration_durations: vec![],
            timing_stats: None,
        };

        let output = format_stress_report(&report);
        assert!(output.contains("Threshold check FAILED"));
    }

    #[test]
    fn format_report_high_cv_warning() {
        let report = StressReport {
            iterations_completed: 10,
            iterations_requested: 10,
            total_duration: Duration::from_secs(5),
            failures: vec![],
            flaky_tests: vec![],
            all_passed: true,
            stopped_early: false,
            threshold_passed: None,
            threshold: None,
            iteration_durations: vec![],
            timing_stats: Some(TimingStats {
                mean_ms: 500.0,
                median_ms: 480.0,
                std_dev_ms: 200.0,
                cv: 0.4, // > 0.3 threshold
                p95_ms: 900.0,
                p99_ms: 950.0,
            }),
        };

        let output = format_stress_report(&report);
        assert!(output.contains("High timing variance"));
    }

    // ─── Parallel workers config ───

    #[test]
    fn parallel_workers_config() {
        let cfg = StressConfig::new(10).with_parallel_workers(4);
        assert_eq!(cfg.parallel_workers, 4);
    }

    // ─── Memory growth safety ───

    #[test]
    fn accumulator_large_iteration_count_no_crash() {
        // Simulate 500 iterations with small test suites to verify
        // the accumulator doesn't crash or cause excessive memory issues
        let cfg = StressConfig::new(500);
        let mut acc = StressAccumulator::new(cfg);

        for _ in 0..500 {
            let result = make_passing_result(5);
            acc.record(result, Duration::from_millis(10));
        }

        assert_eq!(acc.completed(), 500);
        let report = acc.report();
        assert_eq!(report.iterations_completed, 500);
        assert!(report.all_passed);
        assert_eq!(report.iteration_durations.len(), 500);
    }

    #[test]
    fn accumulator_many_tests_per_iteration_no_crash() {
        // Simulate iterations with 200 tests each to check memory with large test suites
        let cfg = StressConfig::new(10);
        let mut acc = StressAccumulator::new(cfg);

        for _ in 0..10 {
            let result = make_passing_result(200);
            acc.record(result, Duration::from_millis(50));
        }

        let report = acc.report();
        assert_eq!(report.iterations_completed, 10);
        assert!(report.all_passed);
    }

    #[test]
    fn accumulator_large_flaky_report_no_crash() {
        // Many flaky tests across many iterations
        let cfg = StressConfig::new(50);
        let mut acc = StressAccumulator::new(cfg);

        for i in 0..50 {
            let mut result = make_passing_result(20);
            // Make every other test fail on even iterations → 10 flaky tests
            if i % 2 == 0 {
                for j in (0..20).step_by(2) {
                    result.suites[0].tests[j].status = TestStatus::Failed;
                    result.suites[0].tests[j].error = Some(TestError {
                        message: "flaky".to_string(),
                        location: None,
                    });
                }
                result.raw_exit_code = 1;
            }
            acc.record(result, Duration::from_millis(10));
        }

        let report = acc.report();
        assert_eq!(report.iterations_completed, 50);
        // Should have detected multiple flaky tests
        assert!(
            !report.flaky_tests.is_empty(),
            "should detect flaky tests across 50 iterations"
        );
        // Each flaky test should have exactly 50 duration entries
        for flaky in &report.flaky_tests {
            assert_eq!(
                flaky.total_runs, 50,
                "each test should have been seen in all 50 iterations"
            );
            assert_eq!(
                flaky.durations.len(),
                50,
                "each flaky test should have 50 duration samples"
            );
        }
    }

    #[test]
    fn accumulator_report_consumes_self() {
        // Verify report() takes ownership (self, not &self), ensuring the
        // large Vec<IterationResult> is freed after report generation
        let cfg = StressConfig::new(3);
        let mut acc = StressAccumulator::new(cfg);
        acc.record(make_passing_result(5), Duration::from_millis(10));
        acc.record(make_passing_result(5), Duration::from_millis(10));
        acc.record(make_passing_result(5), Duration::from_millis(10));

        let _report = acc.report();
        // acc is moved — cannot be used again (compile-time guarantee)
        // The iterations Vec is dropped when report() extracts what it needs
    }

    #[test]
    fn max_duration_stops_accumulation() {
        let cfg = StressConfig::new(1000).with_max_duration(Duration::from_millis(1));
        let mut acc = StressAccumulator::new(cfg);

        // First iteration always succeeds
        acc.record(make_passing_result(5), Duration::from_millis(10));
        // Sleep to exceed max_duration
        std::thread::sleep(Duration::from_millis(5));
        // This should return false (time exceeded)
        let should_continue = acc.record(make_passing_result(5), Duration::from_millis(10));
        assert!(
            !should_continue || acc.is_time_exceeded(),
            "should stop when max_duration exceeded"
        );
    }
}
