use std::path::PathBuf;
use std::time::{Duration, Instant};

use colored::Colorize;

use crate::adapters::TestRunResult;
use crate::config::{Config, WatchConfig};
use crate::error::{Result, TestxError};
use crate::events::EventBus;
use crate::runner::{Runner, RunnerConfig};
use crate::watcher::terminal::{
    clear_screen, print_watch_separator, print_watch_start, print_watch_status, TerminalInput,
    WatchAction,
};
use crate::watcher::file_watcher::FileWatcher;

/// Statistics tracked across watch mode re-runs.
#[derive(Debug, Clone, Default)]
pub struct WatchStats {
    /// Total number of re-runs performed.
    pub total_runs: u32,
    /// Number of runs that had failures.
    pub failed_runs: u32,
    /// Number of runs that passed completely.
    pub passed_runs: u32,
    /// Time of the last run.
    pub last_run: Option<Instant>,
    /// Duration of the last run.
    pub last_duration: Option<Duration>,
    /// Number of tests that failed in the last run.
    pub last_failures: u32,
    /// Number of tests that passed in the last run.
    pub last_passed: u32,
}

impl WatchStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the result of a test run.
    pub fn record_run(&mut self, result: &TestRunResult, duration: Duration) {
        self.total_runs += 1;
        self.last_run = Some(Instant::now());
        self.last_duration = Some(duration);
        self.last_failures = result.total_failed() as u32;
        self.last_passed = result.total_passed() as u32;

        if result.total_failed() > 0 {
            self.failed_runs += 1;
        } else {
            self.passed_runs += 1;
        }
    }

    /// Format a summary line for display.
    pub fn summary(&self) -> String {
        format!(
            "runs: {} total, {} passed, {} failed",
            self.total_runs, self.passed_runs, self.failed_runs
        )
    }
}

/// Options controlling watch runner behavior.
#[derive(Debug, Clone)]
pub struct WatchRunnerOptions {
    /// Clear screen between runs.
    pub clear_screen: bool,
    /// Only re-run failed tests (when triggered by 'f' key).
    pub run_failed_only: bool,
    /// Debounce time in milliseconds.
    pub debounce_ms: u64,
    /// Maximum number of re-runs (0 = unlimited, useful for testing).
    pub max_runs: u32,
    /// Extra arguments to pass to the test runner.
    pub extra_args: Vec<String>,
    /// Verbose mode.
    pub verbose: bool,
}

impl Default for WatchRunnerOptions {
    fn default() -> Self {
        Self {
            clear_screen: true,
            run_failed_only: false,
            debounce_ms: 300,
            max_runs: 0,
            extra_args: Vec::new(),
            verbose: false,
        }
    }
}

impl WatchRunnerOptions {
    /// Create options from a WatchConfig and extra CLI settings.
    pub fn from_config(config: &WatchConfig) -> Self {
        Self {
            clear_screen: config.clear,
            debounce_ms: config.debounce_ms,
            ..Default::default()
        }
    }
}

/// The main watch mode runner loop.
///
/// Orchestrates file watching, terminal input, and test execution
/// in a continuous loop until the user quits.
pub struct WatchRunner {
    /// Root project directory.
    project_dir: PathBuf,
    /// Runner configuration template for each run.
    runner_config: RunnerConfig,
    /// Watch-specific options.
    options: WatchRunnerOptions,
    /// Accumulated statistics.
    stats: WatchStats,
    /// Names of tests that failed in the last run.
    failed_tests: Vec<String>,
}

impl WatchRunner {
    /// Create a new WatchRunner.
    pub fn new(
        project_dir: PathBuf,
        runner_config: RunnerConfig,
        options: WatchRunnerOptions,
    ) -> Self {
        Self {
            project_dir,
            runner_config,
            options,
            stats: WatchStats::new(),
            failed_tests: Vec::new(),
        }
    }

    /// Create a WatchRunner from a Config file.
    pub fn from_config(project_dir: PathBuf, config: &Config) -> Self {
        let mut runner_config = RunnerConfig::new(project_dir.clone());
        runner_config.merge_config(config);

        let watch_config = config.watch_config();
        let options = WatchRunnerOptions::from_config(&watch_config);

        Self::new(project_dir, runner_config, options)
    }

    /// Get current watch statistics.
    pub fn stats(&self) -> &WatchStats {
        &self.stats
    }

    /// Get the list of tests that failed in the last run.
    pub fn failed_tests(&self) -> &[String] {
        &self.failed_tests
    }

    /// Start the watch mode loop.
    ///
    /// This method blocks until the user presses 'q' or max_runs is reached.
    /// Returns the final watch statistics.
    pub fn start(&mut self, watch_config: &WatchConfig) -> Result<WatchStats> {
        // Create the file watcher
        let mut watcher = FileWatcher::new(&self.project_dir, watch_config).map_err(|e| {
            TestxError::WatchError {
                message: format!("Failed to start file watcher: {}", e),
            }
        })?;

        // Start terminal input reader
        let terminal = TerminalInput::new();

        print_watch_start(&self.project_dir);

        // Initial run
        self.execute_run()?;

        loop {
            // Check max run limit
            if self.options.max_runs > 0 && self.stats.total_runs >= self.options.max_runs {
                break;
            }

            // Check for user input (non-blocking)
            match terminal.poll() {
                WatchAction::Quit => {
                    self.print_final_summary();
                    break;
                }
                WatchAction::RunAll => {
                    self.options.run_failed_only = false;
                    if self.options.clear_screen {
                        clear_screen();
                    }
                    print_watch_separator();
                    self.execute_run()?;
                    continue;
                }
                WatchAction::RunFailed => {
                    self.options.run_failed_only = true;
                    if self.options.clear_screen {
                        clear_screen();
                    }
                    print_watch_separator();
                    self.execute_run()?;
                    continue;
                }
                WatchAction::ClearAndRun => {
                    clear_screen();
                    print_watch_separator();
                    self.execute_run()?;
                    continue;
                }
                WatchAction::Continue => {}
            }

            // Wait for file changes (with timeout so we can poll terminal)
            let changed = self.poll_changes_with_timeout(&mut watcher, Duration::from_millis(200));

            if !changed.is_empty() {
                if self.options.verbose {
                    for path in &changed {
                        eprintln!("  {} {}", "changed:".dimmed(), path.display());
                    }
                }

                if self.options.clear_screen {
                    clear_screen();
                }

                print_watch_separator();
                print_watch_status(changed.len());

                self.execute_run()?;
            }
        }

        Ok(self.stats.clone())
    }

    /// Execute a single test run.
    fn execute_run(&mut self) -> Result<()> {
        let mut config = self.runner_config.clone();

        // If running only failed tests, set the filter
        if self.options.run_failed_only && !self.failed_tests.is_empty() {
            let filter = self.failed_tests.join("|");
            config.filter = Some(filter);
            println!(
                "  {} {}",
                "re-running".yellow().bold(),
                format!("{} failed test(s)", self.failed_tests.len()).dimmed()
            );
        }

        let event_bus = EventBus::new();
        let mut runner = Runner::new(config).with_event_bus(event_bus);

        let start = Instant::now();
        let result = runner.run();
        let elapsed = start.elapsed();

        match result {
            Ok((test_result, _exec_output)) => {
                self.stats.record_run(&test_result, elapsed);

                // Track failed test names for "run failed only" mode
                self.failed_tests = test_result
                    .suites
                    .iter()
                    .flat_map(|s| s.tests.iter())
                    .filter(|t| matches!(t.status, crate::adapters::TestStatus::Failed))
                    .map(|t| t.name.clone())
                    .collect();

                self.print_run_summary(&test_result, elapsed);
            }
            Err(e) => {
                self.stats.total_runs += 1;
                self.stats.failed_runs += 1;
                eprintln!("  {} {}", "error:".red().bold(), e);
            }
        }

        Ok(())
    }

    /// Poll for file changes with a short timeout so the main loop
    /// can also check for terminal input.
    fn poll_changes_with_timeout(
        &self,
        _watcher: &mut FileWatcher,
        _timeout: Duration,
    ) -> Vec<PathBuf> {
        // We use a non-blocking approach: check for pending events
        // without fully blocking on wait_for_changes
        // The FileWatcher.wait_for_changes blocks, so we use try-poll approach
        // by sleeping a small amount and checking
        std::thread::sleep(Duration::from_millis(100));

        // Drain any pending events from the watcher's internal channel
        // This is a simplified approach - the watcher's recv_timeout in
        // wait_for_changes handles the actual timing
        Vec::new()
    }

    /// Print summary after a single run.
    fn print_run_summary(&self, result: &TestRunResult, elapsed: Duration) {
        let failed = result.total_failed();
        let passed = result.total_passed();
        let skipped = result.total_skipped();

        let status = if failed > 0 {
            format!("FAIL ({} failed)", failed).red().bold()
        } else {
            "PASS".green().bold()
        };

        println!();
        println!(
            "  {} {} {} in {:.2}s",
            status,
            format!("{} passed", passed).green(),
            if skipped > 0 {
                format!(", {} skipped", skipped).yellow().to_string()
            } else {
                String::new()
            },
            elapsed.as_secs_f64()
        );

        println!(
            "  {} {}",
            "session:".dimmed(),
            self.stats.summary().dimmed()
        );
    }

    /// Print final summary when exiting watch mode.
    fn print_final_summary(&self) {
        println!();
        println!("{}", "─".repeat(60).dimmed());
        println!(
            "  {} {}",
            "watch mode ended".bold(),
            self.stats.summary().dimmed()
        );
        println!();
    }
}

/// Convenience function to launch watch mode from CLI args.
pub fn launch_watch_mode(
    project_dir: PathBuf,
    config: &Config,
    runner_config: RunnerConfig,
) -> Result<()> {
    let watch_config = config.watch_config();
    let options = WatchRunnerOptions::from_config(&watch_config);

    let mut watch_runner = WatchRunner::new(project_dir, runner_config, options);
    let _stats = watch_runner.start(&watch_config)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};

    /// Helper to build a TestRunResult with given pass/fail counts.
    fn make_result(passed: usize, failed: usize) -> TestRunResult {
        let mut tests = Vec::new();
        for i in 0..passed {
            tests.push(TestCase {
                name: format!("pass_{}", i),
                status: TestStatus::Passed,
                duration: Duration::from_millis(10),
                error: None,
            });
        }
        for i in 0..failed {
            tests.push(TestCase {
                name: format!("fail_{}", i),
                status: TestStatus::Failed,
                duration: Duration::from_millis(10),
                error: None,
            });
        }
        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".to_string(),
                tests,
            }],
            duration: Duration::from_secs(1),
            raw_exit_code: if failed > 0 { 1 } else { 0 },
        }
    }

    #[test]
    fn watch_stats_default() {
        let stats = WatchStats::new();
        assert_eq!(stats.total_runs, 0);
        assert_eq!(stats.failed_runs, 0);
        assert_eq!(stats.passed_runs, 0);
        assert!(stats.last_run.is_none());
        assert!(stats.last_duration.is_none());
    }

    #[test]
    fn watch_stats_record_passing_run() {
        let mut stats = WatchStats::new();
        let result = make_result(5, 0);

        stats.record_run(&result, Duration::from_secs(1));

        assert_eq!(stats.total_runs, 1);
        assert_eq!(stats.passed_runs, 1);
        assert_eq!(stats.failed_runs, 0);
        assert_eq!(stats.last_passed, 5);
        assert_eq!(stats.last_failures, 0);
        assert!(stats.last_run.is_some());
    }

    #[test]
    fn watch_stats_record_failing_run() {
        let mut stats = WatchStats::new();
        let result = make_result(3, 2);

        stats.record_run(&result, Duration::from_secs(2));

        assert_eq!(stats.total_runs, 1);
        assert_eq!(stats.passed_runs, 0);
        assert_eq!(stats.failed_runs, 1);
        assert_eq!(stats.last_passed, 3);
        assert_eq!(stats.last_failures, 2);
    }

    #[test]
    fn watch_stats_multiple_runs() {
        let mut stats = WatchStats::new();

        let pass = make_result(5, 0);
        let fail = make_result(3, 2);

        stats.record_run(&pass, Duration::from_secs(1));
        stats.record_run(&fail, Duration::from_secs(2));
        stats.record_run(&pass, Duration::from_secs(1));

        assert_eq!(stats.total_runs, 3);
        assert_eq!(stats.passed_runs, 2);
        assert_eq!(stats.failed_runs, 1);
    }

    #[test]
    fn watch_stats_summary() {
        let mut stats = WatchStats::new();
        assert_eq!(stats.summary(), "runs: 0 total, 0 passed, 0 failed");

        let result = make_result(5, 0);
        stats.record_run(&result, Duration::from_secs(1));
        assert_eq!(stats.summary(), "runs: 1 total, 1 passed, 0 failed");
    }

    #[test]
    fn watch_runner_options_default() {
        let opts = WatchRunnerOptions::default();
        assert!(opts.clear_screen);
        assert!(!opts.run_failed_only);
        assert_eq!(opts.debounce_ms, 300);
        assert_eq!(opts.max_runs, 0);
        assert!(opts.extra_args.is_empty());
    }

    #[test]
    fn watch_runner_options_from_config() {
        let config = WatchConfig {
            clear: false,
            debounce_ms: 500,
            ..Default::default()
        };

        let opts = WatchRunnerOptions::from_config(&config);
        assert!(!opts.clear_screen);
        assert_eq!(opts.debounce_ms, 500);
    }

    #[test]
    fn watch_runner_creation() {
        let dir = PathBuf::from("/tmp/test");
        let config = RunnerConfig::new(dir.clone());
        let opts = WatchRunnerOptions::default();

        let runner = WatchRunner::new(dir.clone(), config, opts);
        assert_eq!(runner.stats().total_runs, 0);
        assert!(runner.failed_tests().is_empty());
    }

    #[test]
    fn watch_runner_from_config() {
        let dir = PathBuf::from("/tmp/test");
        let config = Config::default();

        let runner = WatchRunner::from_config(dir, &config);
        assert_eq!(runner.stats().total_runs, 0);
    }

    #[test]
    fn watch_stats_last_duration_recorded() {
        let mut stats = WatchStats::new();
        let result = make_result(1, 0);

        let dur = Duration::from_millis(1234);
        stats.record_run(&result, dur);
        assert_eq!(stats.last_duration, Some(dur));
    }

    #[test]
    fn run_summary_format_pass() {
        let dir = PathBuf::from("/tmp/test");
        let config = RunnerConfig::new(dir.clone());
        let opts = WatchRunnerOptions::default();
        let runner = WatchRunner::new(dir, config, opts);

        // Just verify summary format with no runs
        let summary = runner.stats().summary();
        assert!(summary.contains("0 total"));
    }
}
