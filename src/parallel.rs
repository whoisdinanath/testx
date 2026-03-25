use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::{TestCase, TestRunResult, TestSuite};

/// Configuration for parallel test execution.
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Maximum number of concurrent jobs (0 = auto-detect)
    pub max_jobs: usize,
    /// How to distribute tests across workers
    pub strategy: PartitionStrategy,
    /// Whether to fail fast (stop all workers on first failure)
    pub fail_fast: bool,
    /// Whether to isolate output per worker
    pub isolate_output: bool,
}

impl ParallelConfig {
    /// Create a new parallel config with auto-detected job count.
    pub fn new() -> Self {
        Self {
            max_jobs: 0,
            strategy: PartitionStrategy::RoundRobin,
            fail_fast: false,
            isolate_output: true,
        }
    }

    /// Set max jobs.
    pub fn with_max_jobs(mut self, jobs: usize) -> Self {
        self.max_jobs = jobs;
        self
    }

    /// Set partition strategy.
    pub fn with_strategy(mut self, strategy: PartitionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set fail-fast mode.
    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    /// Get effective job count — uses available CPUs if max_jobs is 0.
    pub fn effective_jobs(&self) -> usize {
        if self.max_jobs == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            self.max_jobs
        }
    }

    /// Whether parallel execution is enabled.
    pub fn is_enabled(&self) -> bool {
        self.effective_jobs() > 1
    }
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Strategy for partitioning tests across workers.
#[derive(Debug, Clone)]
pub enum PartitionStrategy {
    /// Distribute tests in round-robin order
    RoundRobin,
    /// Group tests by suite
    BySuite,
    /// Distribute by estimated duration (longest first)
    ByDuration,
    /// Each worker gets a contiguous chunk
    Chunked,
}

/// A partition of work to be executed by a single worker.
#[derive(Debug, Clone)]
pub struct WorkPartition {
    /// Worker index (0-based)
    pub worker_id: usize,
    /// Suite name to test name mapping for this partition
    pub test_groups: Vec<TestGroup>,
}

/// A group of tests from the same suite.
#[derive(Debug, Clone)]
pub struct TestGroup {
    /// Suite name
    pub suite_name: String,
    /// Test names in this group
    pub test_names: Vec<String>,
}

impl WorkPartition {
    /// Total number of tests in this partition.
    pub fn total_tests(&self) -> usize {
        self.test_groups.iter().map(|g| g.test_names.len()).sum()
    }

    /// Whether this partition is empty.
    pub fn is_empty(&self) -> bool {
        self.test_groups.is_empty()
    }
}

/// Partition test cases across N workers using the given strategy.
pub fn partition_tests(
    result: &TestRunResult,
    num_workers: usize,
    strategy: &PartitionStrategy,
) -> Vec<WorkPartition> {
    if num_workers == 0 {
        return vec![];
    }

    match strategy {
        PartitionStrategy::RoundRobin => partition_round_robin(result, num_workers),
        PartitionStrategy::BySuite => partition_by_suite(result, num_workers),
        PartitionStrategy::ByDuration => partition_by_duration(result, num_workers),
        PartitionStrategy::Chunked => partition_chunked(result, num_workers),
    }
}

fn partition_round_robin(result: &TestRunResult, num_workers: usize) -> Vec<WorkPartition> {
    let mut partitions: Vec<WorkPartition> = (0..num_workers)
        .map(|id| WorkPartition {
            worker_id: id,
            test_groups: Vec::new(),
        })
        .collect();

    let mut worker_idx = 0;
    for suite in &result.suites {
        for test in &suite.tests {
            let partition = &mut partitions[worker_idx % num_workers];

            // Find or create test group for this suite
            if let Some(group) = partition
                .test_groups
                .iter_mut()
                .find(|g| g.suite_name == suite.name)
            {
                group.test_names.push(test.name.clone());
            } else {
                partition.test_groups.push(TestGroup {
                    suite_name: suite.name.clone(),
                    test_names: vec![test.name.clone()],
                });
            }

            worker_idx += 1;
        }
    }

    partitions
}

fn partition_by_suite(result: &TestRunResult, num_workers: usize) -> Vec<WorkPartition> {
    let mut partitions: Vec<WorkPartition> = (0..num_workers)
        .map(|id| WorkPartition {
            worker_id: id,
            test_groups: Vec::new(),
        })
        .collect();

    // Assign each suite to the worker with the least tests
    for suite in &result.suites {
        let min_worker = partitions
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.total_tests())
            .map(|(i, _)| i)
            .unwrap_or(0);

        partitions[min_worker].test_groups.push(TestGroup {
            suite_name: suite.name.clone(),
            test_names: suite.tests.iter().map(|t| t.name.clone()).collect(),
        });
    }

    partitions
}

fn partition_by_duration(result: &TestRunResult, num_workers: usize) -> Vec<WorkPartition> {
    let mut partitions: Vec<WorkPartition> = (0..num_workers)
        .map(|id| WorkPartition {
            worker_id: id,
            test_groups: Vec::new(),
        })
        .collect();

    // Collect all tests with their durations, sorted longest first
    let mut all_tests: Vec<(&str, &str, Duration)> = Vec::new();
    for suite in &result.suites {
        for test in &suite.tests {
            all_tests.push((&suite.name, &test.name, test.duration));
        }
    }
    all_tests.sort_by(|a, b| b.2.cmp(&a.2));

    // Track total duration per worker
    let mut worker_durations = vec![Duration::ZERO; num_workers];

    // Greedy assignment: assign longest test to least-loaded worker
    for (suite_name, test_name, duration) in all_tests {
        let min_worker = worker_durations
            .iter()
            .enumerate()
            .min_by_key(|(_, d)| *d)
            .map(|(i, _)| i)
            .unwrap_or(0);

        worker_durations[min_worker] += duration;

        let partition = &mut partitions[min_worker];
        if let Some(group) = partition
            .test_groups
            .iter_mut()
            .find(|g| g.suite_name == suite_name)
        {
            group.test_names.push(test_name.to_string());
        } else {
            partition.test_groups.push(TestGroup {
                suite_name: suite_name.to_string(),
                test_names: vec![test_name.to_string()],
            });
        }
    }

    partitions
}

fn partition_chunked(result: &TestRunResult, num_workers: usize) -> Vec<WorkPartition> {
    let mut partitions: Vec<WorkPartition> = (0..num_workers)
        .map(|id| WorkPartition {
            worker_id: id,
            test_groups: Vec::new(),
        })
        .collect();

    // Flatten all tests
    let mut all_tests: Vec<(&str, &str)> = Vec::new();
    for suite in &result.suites {
        for test in &suite.tests {
            all_tests.push((&suite.name, &test.name));
        }
    }

    let chunk_size = all_tests.len().div_ceil(num_workers);

    for (i, chunk) in all_tests.chunks(chunk_size).enumerate() {
        if i >= num_workers {
            break;
        }
        for (suite_name, test_name) in chunk {
            let partition = &mut partitions[i];
            if let Some(group) = partition
                .test_groups
                .iter_mut()
                .find(|g| g.suite_name == *suite_name)
            {
                group.test_names.push(test_name.to_string());
            } else {
                partition.test_groups.push(TestGroup {
                    suite_name: suite_name.to_string(),
                    test_names: vec![test_name.to_string()],
                });
            }
        }
    }

    partitions
}

/// Result from a single parallel worker.
#[derive(Debug, Clone)]
pub struct WorkerResult {
    /// Worker index
    pub worker_id: usize,
    /// Test results from this worker
    pub result: TestRunResult,
    /// Wall time for this worker
    pub wall_time: Duration,
    /// Whether this worker was cancelled due to fail-fast
    pub cancelled: bool,
}

/// Aggregated result from all parallel workers.
#[derive(Debug, Clone)]
pub struct ParallelResult {
    /// Individual worker results
    pub workers: Vec<WorkerResult>,
    /// Merged result from all workers
    pub merged: TestRunResult,
    /// Total wall time (max of all workers)
    pub wall_time: Duration,
    /// Number of workers used
    pub num_workers: usize,
    /// Whether any worker was cancelled
    pub had_cancellation: bool,
}

/// Merge results from multiple workers into a single TestRunResult.
pub fn merge_worker_results(workers: &[WorkerResult]) -> TestRunResult {
    let mut suite_map: HashMap<String, Vec<TestCase>> = HashMap::new();
    let mut total_duration = Duration::ZERO;
    let mut any_failed = false;

    for worker in workers {
        total_duration = total_duration.max(worker.wall_time);
        for suite in &worker.result.suites {
            let tests = suite_map.entry(suite.name.clone()).or_default();
            tests.extend(suite.tests.iter().cloned());
        }
        if worker.result.total_failed() > 0 {
            any_failed = true;
        }
    }

    let suites: Vec<TestSuite> = suite_map
        .into_iter()
        .map(|(name, tests)| TestSuite { name, tests })
        .collect();

    TestRunResult {
        suites,
        duration: total_duration,
        raw_exit_code: if any_failed { 1 } else { 0 },
    }
}

/// Build a ParallelResult from worker results.
pub fn build_parallel_result(workers: Vec<WorkerResult>) -> ParallelResult {
    let wall_time = workers.iter().map(|w| w.wall_time).max().unwrap_or_default();
    let num_workers = workers.len();
    let had_cancellation = workers.iter().any(|w| w.cancelled);
    let merged = merge_worker_results(&workers);

    ParallelResult {
        workers,
        merged,
        wall_time,
        num_workers,
        had_cancellation,
    }
}

/// Thread-safe cancellation flag for fail-fast mode.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<Mutex<bool>>,
}

impl CancellationToken {
    /// Create a new cancellation token.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(Mutex::new(false)),
        }
    }

    /// Cancel all workers.
    pub fn cancel(&self) {
        if let Ok(mut c) = self.cancelled.lock() {
            *c = true;
        }
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
            .lock()
            .map(|c| *c)
            .unwrap_or(false)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about parallel execution.
#[derive(Debug, Clone)]
pub struct ParallelStats {
    /// Number of workers used
    pub num_workers: usize,
    /// Tests per worker (min, max, avg)
    pub tests_per_worker: (usize, usize, f64),
    /// Total CPU time (sum of all workers)
    pub total_cpu_time: Duration,
    /// Wall time
    pub wall_time: Duration,
    /// Speedup factor (cpu_time / wall_time)
    pub speedup: f64,
    /// Efficiency (speedup / num_workers)
    pub efficiency: f64,
}

/// Compute statistics about parallel execution.
pub fn compute_parallel_stats(result: &ParallelResult) -> ParallelStats {
    let num_workers = result.num_workers;
    let total_cpu_time: Duration = result.workers.iter().map(|w| w.wall_time).sum();
    let wall_time = result.wall_time;

    let tests_per_worker: Vec<usize> = result
        .workers
        .iter()
        .map(|w| w.result.total_tests())
        .collect();

    let min_tests = tests_per_worker.iter().copied().min().unwrap_or(0);
    let max_tests = tests_per_worker.iter().copied().max().unwrap_or(0);
    let avg_tests = if num_workers > 0 {
        tests_per_worker.iter().sum::<usize>() as f64 / num_workers as f64
    } else {
        0.0
    };

    let speedup = if wall_time.as_secs_f64() > 0.0 {
        total_cpu_time.as_secs_f64() / wall_time.as_secs_f64()
    } else {
        1.0
    };

    let efficiency = if num_workers > 0 {
        speedup / num_workers as f64
    } else {
        0.0
    };

    ParallelStats {
        num_workers,
        tests_per_worker: (min_tests, max_tests, avg_tests),
        total_cpu_time,
        wall_time,
        speedup,
        efficiency,
    }
}

/// Estimate how long a partition would take based on known durations.
pub fn estimate_partition_time(partition: &WorkPartition, result: &TestRunResult) -> Duration {
    let mut total = Duration::ZERO;

    for group in &partition.test_groups {
        if let Some(suite) = result.suites.iter().find(|s| s.name == group.suite_name) {
            for test_name in &group.test_names {
                if let Some(test) = suite.tests.iter().find(|t| &t.name == test_name) {
                    total += test.duration;
                }
            }
        }
    }

    total
}

/// Check if partitions are balanced (no worker has more than 2x the average).
pub fn is_balanced(partitions: &[WorkPartition]) -> bool {
    if partitions.is_empty() {
        return true;
    }

    let counts: Vec<usize> = partitions.iter().map(|p| p.total_tests()).collect();
    let min = counts.iter().copied().min().unwrap_or(0);
    let max = counts.iter().copied().max().unwrap_or(0);

    if max == 0 {
        return true;
    }

    // Balanced if the max worker has no more than 2x the min (or min+2 for small counts)
    max <= min * 2 + 2
}

/// Rebalance partitions that are too skewed.
pub fn rebalance(partitions: &mut [WorkPartition]) {
    if partitions.is_empty() {
        return;
    }

    // Flatten all tests
    let mut all_tests: Vec<(String, String)> = Vec::new();
    for partition in partitions.iter() {
        for group in &partition.test_groups {
            for test_name in &group.test_names {
                all_tests.push((group.suite_name.clone(), test_name.clone()));
            }
        }
    }

    // Clear all partitions
    for partition in partitions.iter_mut() {
        partition.test_groups.clear();
    }

    // Redistribute round-robin
    for (i, (suite_name, test_name)) in all_tests.iter().enumerate() {
        let idx = i % partitions.len();
        let partition = &mut partitions[idx];

        if let Some(group) = partition
            .test_groups
            .iter_mut()
            .find(|g| g.suite_name == *suite_name)
        {
            group.test_names.push(test_name.clone());
        } else {
            partition.test_groups.push(TestGroup {
                suite_name: suite_name.clone(),
                test_names: vec![test_name.clone()],
            });
        }
    }
}

/// Format a partition for display.
pub fn format_partition(partition: &WorkPartition) -> String {
    let mut parts = Vec::new();
    for group in &partition.test_groups {
        parts.push(format!(
            "{}({} tests)",
            group.suite_name,
            group.test_names.len()
        ));
    }
    format!(
        "Worker {}: {} tests [{}]",
        partition.worker_id,
        partition.total_tests(),
        parts.join(", ")
    )
}

/// Monitor for tracking parallel execution progress.
#[derive(Debug)]
pub struct ProgressMonitor {
    /// Start time of execution
    start_time: Instant,
    /// Total tests to run
    total_tests: usize,
    /// Completed test count per worker
    completed: Arc<Mutex<HashMap<usize, usize>>>,
}

impl ProgressMonitor {
    /// Create a new progress monitor.
    pub fn new(total_tests: usize) -> Self {
        Self {
            start_time: Instant::now(),
            total_tests,
            completed: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record a completed test for a worker.
    pub fn record_completion(&self, worker_id: usize) {
        if let Ok(mut map) = self.completed.lock() {
            *map.entry(worker_id).or_insert(0) += 1;
        }
    }

    /// Get total completed tests across all workers.
    pub fn total_completed(&self) -> usize {
        self.completed
            .lock()
            .map(|map| map.values().sum())
            .unwrap_or(0)
    }

    /// Get completion percentage.
    pub fn progress_percent(&self) -> f64 {
        if self.total_tests == 0 {
            return 100.0;
        }
        (self.total_completed() as f64 / self.total_tests as f64) * 100.0
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Estimated time remaining.
    pub fn eta(&self) -> Option<Duration> {
        let completed = self.total_completed();
        if completed == 0 {
            return None;
        }

        let elapsed = self.elapsed();
        let rate = completed as f64 / elapsed.as_secs_f64();
        let remaining = self.total_tests.saturating_sub(completed) as f64;

        Some(Duration::from_secs_f64(remaining / rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TestStatus;

    fn make_test(name: &str, status: TestStatus, duration_ms: u64) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(duration_ms),
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
            raw_exit_code: 0,
        }
    }

    // ─── ParallelConfig Tests ───────────────────────────────────────────

    #[test]
    fn config_default() {
        let config = ParallelConfig::new();
        assert_eq!(config.max_jobs, 0);
        assert!(!config.fail_fast);
        assert!(config.isolate_output);
    }

    #[test]
    fn config_effective_jobs() {
        let config = ParallelConfig::new().with_max_jobs(4);
        assert_eq!(config.effective_jobs(), 4);
    }

    #[test]
    fn config_auto_detect_jobs() {
        let config = ParallelConfig::new();
        assert!(config.effective_jobs() >= 1);
    }

    #[test]
    fn config_is_enabled() {
        let config = ParallelConfig::new().with_max_jobs(1);
        assert!(!config.is_enabled());

        let config = ParallelConfig::new().with_max_jobs(4);
        assert!(config.is_enabled());
    }

    // ─── Round Robin Partitioning ───────────────────────────────────────

    #[test]
    fn partition_rr_basic() {
        let result = make_result(vec![make_suite(
            "math",
            vec![
                make_test("test_a", TestStatus::Passed, 10),
                make_test("test_b", TestStatus::Passed, 20),
                make_test("test_c", TestStatus::Passed, 30),
                make_test("test_d", TestStatus::Passed, 40),
            ],
        )]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::RoundRobin);
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].total_tests(), 2);
        assert_eq!(partitions[1].total_tests(), 2);
    }

    #[test]
    fn partition_rr_uneven() {
        let result = make_result(vec![make_suite(
            "math",
            vec![
                make_test("test_a", TestStatus::Passed, 10),
                make_test("test_b", TestStatus::Passed, 20),
                make_test("test_c", TestStatus::Passed, 30),
            ],
        )]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::RoundRobin);
        assert_eq!(partitions[0].total_tests(), 2);
        assert_eq!(partitions[1].total_tests(), 1);
    }

    #[test]
    fn partition_rr_more_workers_than_tests() {
        let result = make_result(vec![make_suite(
            "math",
            vec![make_test("test_a", TestStatus::Passed, 10)],
        )]);

        let partitions = partition_tests(&result, 4, &PartitionStrategy::RoundRobin);
        assert_eq!(partitions.len(), 4);
        assert_eq!(partitions[0].total_tests(), 1);
        assert_eq!(partitions[1].total_tests(), 0);
    }

    // ─── By Suite Partitioning ──────────────────────────────────────────

    #[test]
    fn partition_by_suite_basic() {
        let result = make_result(vec![
            make_suite(
                "math",
                vec![
                    make_test("test_add", TestStatus::Passed, 10),
                    make_test("test_sub", TestStatus::Passed, 20),
                ],
            ),
            make_suite(
                "strings",
                vec![
                    make_test("test_concat", TestStatus::Passed, 10),
                    make_test("test_upper", TestStatus::Passed, 20),
                ],
            ),
        ]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::BySuite);
        // Each suite should go to a different worker
        assert_eq!(partitions.len(), 2);
        let total: usize = partitions.iter().map(|p| p.total_tests()).sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn partition_by_suite_unbalanced() {
        let result = make_result(vec![
            make_suite(
                "big",
                vec![
                    make_test("a", TestStatus::Passed, 10),
                    make_test("b", TestStatus::Passed, 10),
                    make_test("c", TestStatus::Passed, 10),
                ],
            ),
            make_suite("small", vec![make_test("d", TestStatus::Passed, 10)]),
        ]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::BySuite);
        // Big suite goes first, small suite to worker with fewer tests
        let total: usize = partitions.iter().map(|p| p.total_tests()).sum();
        assert_eq!(total, 4);
    }

    // ─── By Duration Partitioning ───────────────────────────────────────

    #[test]
    fn partition_by_duration() {
        let result = make_result(vec![make_suite(
            "math",
            vec![
                make_test("slow", TestStatus::Passed, 1000),
                make_test("medium", TestStatus::Passed, 500),
                make_test("fast1", TestStatus::Passed, 100),
                make_test("fast2", TestStatus::Passed, 100),
            ],
        )]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::ByDuration);
        assert_eq!(partitions.len(), 2);

        // Worker 0 gets "slow" (1000ms), worker 1 gets "medium" (500ms)
        // Then "fast1" goes to worker 1 (500+100=600 < 1000), "fast2" to worker 1 (700 < 1000)
        let total: usize = partitions.iter().map(|p| p.total_tests()).sum();
        assert_eq!(total, 4);
    }

    // ─── Chunked Partitioning ───────────────────────────────────────────

    #[test]
    fn partition_chunked_basic() {
        let result = make_result(vec![make_suite(
            "math",
            vec![
                make_test("a", TestStatus::Passed, 10),
                make_test("b", TestStatus::Passed, 10),
                make_test("c", TestStatus::Passed, 10),
                make_test("d", TestStatus::Passed, 10),
            ],
        )]);

        let partitions = partition_tests(&result, 2, &PartitionStrategy::Chunked);
        assert_eq!(partitions[0].total_tests(), 2);
        assert_eq!(partitions[1].total_tests(), 2);
    }

    // ─── Zero Workers ───────────────────────────────────────────────────

    #[test]
    fn partition_zero_workers() {
        let result = make_result(vec![]);
        let partitions = partition_tests(&result, 0, &PartitionStrategy::RoundRobin);
        assert!(partitions.is_empty());
    }

    // ─── Merge Worker Results ───────────────────────────────────────────

    #[test]
    fn merge_workers_basic() {
        let w1 = WorkerResult {
            worker_id: 0,
            result: make_result(vec![make_suite(
                "math",
                vec![make_test("test_add", TestStatus::Passed, 10)],
            )]),
            wall_time: Duration::from_millis(100),
            cancelled: false,
        };

        let w2 = WorkerResult {
            worker_id: 1,
            result: make_result(vec![make_suite(
                "math",
                vec![make_test("test_sub", TestStatus::Passed, 10)],
            )]),
            wall_time: Duration::from_millis(150),
            cancelled: false,
        };

        let merged = merge_worker_results(&[w1, w2]);
        assert_eq!(merged.total_tests(), 2);
        assert_eq!(merged.duration, Duration::from_millis(150)); // max wall time
        assert_eq!(merged.raw_exit_code, 0);
    }

    #[test]
    fn merge_workers_with_failure() {
        let w1 = WorkerResult {
            worker_id: 0,
            result: make_result(vec![make_suite(
                "math",
                vec![make_test("test_add", TestStatus::Failed, 10)],
            )]),
            wall_time: Duration::from_millis(100),
            cancelled: false,
        };

        let w2 = WorkerResult {
            worker_id: 1,
            result: make_result(vec![make_suite(
                "strings",
                vec![make_test("test_concat", TestStatus::Passed, 10)],
            )]),
            wall_time: Duration::from_millis(100),
            cancelled: false,
        };

        let merged = merge_worker_results(&[w1, w2]);
        assert_eq!(merged.total_tests(), 2);
        assert_eq!(merged.raw_exit_code, 1);
    }

    #[test]
    fn merge_workers_same_suite() {
        let w1 = WorkerResult {
            worker_id: 0,
            result: make_result(vec![make_suite(
                "math",
                vec![make_test("test_a", TestStatus::Passed, 10)],
            )]),
            wall_time: Duration::from_millis(100),
            cancelled: false,
        };

        let w2 = WorkerResult {
            worker_id: 1,
            result: make_result(vec![make_suite(
                "math",
                vec![make_test("test_b", TestStatus::Passed, 10)],
            )]),
            wall_time: Duration::from_millis(100),
            cancelled: false,
        };

        let merged = merge_worker_results(&[w1, w2]);
        assert_eq!(merged.suites.len(), 1);
        assert_eq!(merged.suites[0].tests.len(), 2);
    }

    // ─── Build Parallel Result ──────────────────────────────────────────

    #[test]
    fn build_parallel_result_basic() {
        let workers = vec![
            WorkerResult {
                worker_id: 0,
                result: make_result(vec![make_suite(
                    "a",
                    vec![make_test("t1", TestStatus::Passed, 10)],
                )]),
                wall_time: Duration::from_millis(100),
                cancelled: false,
            },
            WorkerResult {
                worker_id: 1,
                result: make_result(vec![make_suite(
                    "b",
                    vec![make_test("t2", TestStatus::Passed, 10)],
                )]),
                wall_time: Duration::from_millis(200),
                cancelled: false,
            },
        ];

        let result = build_parallel_result(workers);
        assert_eq!(result.num_workers, 2);
        assert_eq!(result.wall_time, Duration::from_millis(200));
        assert!(!result.had_cancellation);
        assert_eq!(result.merged.total_tests(), 2);
    }

    #[test]
    fn build_parallel_result_with_cancel() {
        let workers = vec![WorkerResult {
            worker_id: 0,
            result: make_result(vec![]),
            wall_time: Duration::from_millis(50),
            cancelled: true,
        }];

        let result = build_parallel_result(workers);
        assert!(result.had_cancellation);
    }

    // ─── CancellationToken Tests ────────────────────────────────────────

    #[test]
    fn cancellation_token_default() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone() {
        let token = CancellationToken::new();
        let clone = token.clone();
        token.cancel();
        assert!(clone.is_cancelled());
    }

    // ─── Stats Tests ────────────────────────────────────────────────────

    #[test]
    fn stats_basic() {
        let workers = vec![
            WorkerResult {
                worker_id: 0,
                result: make_result(vec![make_suite(
                    "a",
                    vec![
                        make_test("t1", TestStatus::Passed, 10),
                        make_test("t2", TestStatus::Passed, 10),
                    ],
                )]),
                wall_time: Duration::from_millis(100),
                cancelled: false,
            },
            WorkerResult {
                worker_id: 1,
                result: make_result(vec![make_suite(
                    "b",
                    vec![make_test("t3", TestStatus::Passed, 10)],
                )]),
                wall_time: Duration::from_millis(100),
                cancelled: false,
            },
        ];

        let result = build_parallel_result(workers);
        let stats = compute_parallel_stats(&result);

        assert_eq!(stats.num_workers, 2);
        assert_eq!(stats.tests_per_worker.0, 1); // min
        assert_eq!(stats.tests_per_worker.1, 2); // max
        assert!(stats.speedup >= 1.0);
    }

    // ─── Balance Tests ──────────────────────────────────────────────────

    #[test]
    fn is_balanced_basic() {
        let partitions = vec![
            WorkPartition {
                worker_id: 0,
                test_groups: vec![TestGroup {
                    suite_name: "s".into(),
                    test_names: vec!["a".into(), "b".into()],
                }],
            },
            WorkPartition {
                worker_id: 1,
                test_groups: vec![TestGroup {
                    suite_name: "s".into(),
                    test_names: vec!["c".into(), "d".into()],
                }],
            },
        ];
        assert!(is_balanced(&partitions));
    }

    #[test]
    fn is_balanced_skewed() {
        let partitions = vec![
            WorkPartition {
                worker_id: 0,
                test_groups: vec![TestGroup {
                    suite_name: "s".into(),
                    test_names: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into(), "f".into(), "g".into(), "h".into(), "i".into()],
                }],
            },
            WorkPartition {
                worker_id: 1,
                test_groups: vec![TestGroup {
                    suite_name: "s".into(),
                    test_names: vec!["f".into()],
                }],
            },
        ];
        assert!(!is_balanced(&partitions));
    }

    #[test]
    fn is_balanced_empty() {
        assert!(is_balanced(&[]));
    }

    // ─── Rebalance Tests ────────────────────────────────────────────────

    #[test]
    fn rebalance_skewed() {
        let mut partitions = vec![
            WorkPartition {
                worker_id: 0,
                test_groups: vec![TestGroup {
                    suite_name: "s".into(),
                    test_names: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                }],
            },
            WorkPartition {
                worker_id: 1,
                test_groups: Vec::new(),
            },
        ];

        rebalance(&mut partitions);
        assert_eq!(partitions[0].total_tests(), 2);
        assert_eq!(partitions[1].total_tests(), 2);
    }

    // ─── Estimate Time Tests ────────────────────────────────────────────

    #[test]
    fn estimate_time() {
        let result = make_result(vec![make_suite(
            "math",
            vec![
                make_test("a", TestStatus::Passed, 100),
                make_test("b", TestStatus::Passed, 200),
            ],
        )]);

        let partition = WorkPartition {
            worker_id: 0,
            test_groups: vec![TestGroup {
                suite_name: "math".into(),
                test_names: vec!["a".into(), "b".into()],
            }],
        };

        let est = estimate_partition_time(&partition, &result);
        assert_eq!(est, Duration::from_millis(300));
    }

    #[test]
    fn estimate_time_missing_test() {
        let result = make_result(vec![make_suite(
            "math",
            vec![make_test("a", TestStatus::Passed, 100)],
        )]);

        let partition = WorkPartition {
            worker_id: 0,
            test_groups: vec![TestGroup {
                suite_name: "math".into(),
                test_names: vec!["a".into(), "nonexistent".into()],
            }],
        };

        let est = estimate_partition_time(&partition, &result);
        assert_eq!(est, Duration::from_millis(100)); // only "a"
    }

    // ─── Format Partition ───────────────────────────────────────────────

    #[test]
    fn format_partition_test() {
        let partition = WorkPartition {
            worker_id: 0,
            test_groups: vec![
                TestGroup {
                    suite_name: "math".into(),
                    test_names: vec!["a".into(), "b".into()],
                },
                TestGroup {
                    suite_name: "strings".into(),
                    test_names: vec!["c".into()],
                },
            ],
        };

        let formatted = format_partition(&partition);
        assert!(formatted.contains("Worker 0"));
        assert!(formatted.contains("3 tests"));
        assert!(formatted.contains("math(2 tests)"));
        assert!(formatted.contains("strings(1 tests)"));
    }

    // ─── Progress Monitor Tests ─────────────────────────────────────────

    #[test]
    fn progress_monitor_basic() {
        let monitor = ProgressMonitor::new(10);
        assert_eq!(monitor.total_completed(), 0);
        assert_eq!(monitor.progress_percent(), 0.0);
    }

    #[test]
    fn progress_monitor_track() {
        let monitor = ProgressMonitor::new(4);
        monitor.record_completion(0);
        monitor.record_completion(0);
        monitor.record_completion(1);

        assert_eq!(monitor.total_completed(), 3);
        assert_eq!(monitor.progress_percent(), 75.0);
    }

    #[test]
    fn progress_monitor_zero_total() {
        let monitor = ProgressMonitor::new(0);
        assert_eq!(monitor.progress_percent(), 100.0);
    }

    #[test]
    fn progress_monitor_elapsed() {
        let monitor = ProgressMonitor::new(10);
        assert!(monitor.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn progress_monitor_eta_none() {
        let monitor = ProgressMonitor::new(10);
        assert!(monitor.eta().is_none()); // no completions yet
    }
}
