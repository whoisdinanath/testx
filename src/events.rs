use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::{TestCase, TestRunResult, TestSuite};

/// Events emitted during test execution, enabling decoupled output rendering.
#[derive(Debug, Clone)]
pub enum TestEvent {
    /// Test run is starting.
    RunStarted {
        adapter: String,
        framework: String,
        project_dir: PathBuf,
    },

    /// A test suite has started executing.
    SuiteStarted { name: String },

    /// A single test has started.
    TestStarted { suite: String, name: String },

    /// A single test has completed.
    TestFinished { suite: String, test: TestCase },

    /// An entire suite has completed.
    SuiteFinished { suite: TestSuite },

    /// The entire test run has completed.
    RunFinished { result: TestRunResult },

    /// Raw output line from the test process.
    RawOutput { stream: Stream, line: String },

    /// Watch mode: files changed, triggering re-run.
    WatchRerun { changed_files: Vec<PathBuf> },

    /// Retry: a failed test is being retried.
    RetryStarted {
        test_name: String,
        attempt: u32,
        max_attempts: u32,
    },

    /// Retry: attempt completed.
    RetryFinished {
        test_name: String,
        attempt: u32,
        passed: bool,
    },

    /// Filter applied to test run.
    FilterApplied {
        pattern: String,
        matched_count: usize,
    },

    /// Parallel: an adapter run started.
    ParallelAdapterStarted { adapter: String },

    /// Parallel: an adapter run finished.
    ParallelAdapterFinished {
        adapter: String,
        result: TestRunResult,
    },

    /// A warning message (non-fatal).
    Warning { message: String },

    /// A progress tick (for long-running operations).
    Progress {
        message: String,
        current: usize,
        total: usize,
    },
}

/// Output stream identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Handler for test events. Implement this to create custom output formatters.
pub trait EventHandler: Send {
    /// Handle a single test event.
    fn handle(&mut self, event: &TestEvent);

    /// Called when a batch of events is complete (e.g., end of run).
    fn flush(&mut self) {}
}

/// Event bus that distributes events to all registered handlers.
pub struct EventBus {
    handlers: Vec<Box<dyn EventHandler>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register an event handler.
    pub fn subscribe(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
    }

    /// Emit an event to all registered handlers.
    pub fn emit(&mut self, event: TestEvent) {
        for handler in &mut self.handlers {
            handler.handle(&event);
        }
    }

    /// Flush all handlers (call at end of run).
    pub fn flush(&mut self) {
        for handler in &mut self.handlers {
            handler.flush();
        }
    }

    /// Returns the number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

/// A handler that collects all events into a vec for testing.
pub struct CollectingHandler {
    pub events: Vec<TestEvent>,
}

impl CollectingHandler {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl Default for CollectingHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler for CollectingHandler {
    fn handle(&mut self, event: &TestEvent) {
        self.events.push(event.clone());
    }
}

/// A handler that counts events by type for quick assertions.
#[derive(Debug, Default)]
pub struct CountingHandler {
    pub run_started: usize,
    pub suite_started: usize,
    pub test_started: usize,
    pub test_finished: usize,
    pub suite_finished: usize,
    pub run_finished: usize,
    pub raw_output: usize,
    pub warnings: usize,
    pub total: usize,
}

impl EventHandler for CountingHandler {
    fn handle(&mut self, event: &TestEvent) {
        self.total += 1;
        match event {
            TestEvent::RunStarted { .. } => self.run_started += 1,
            TestEvent::SuiteStarted { .. } => self.suite_started += 1,
            TestEvent::TestStarted { .. } => self.test_started += 1,
            TestEvent::TestFinished { .. } => self.test_finished += 1,
            TestEvent::SuiteFinished { .. } => self.suite_finished += 1,
            TestEvent::RunFinished { .. } => self.run_finished += 1,
            TestEvent::RawOutput { .. } => self.raw_output += 1,
            TestEvent::Warning { .. } => self.warnings += 1,
            _ => {}
        }
    }
}

/// A handler that writes raw output lines to a buffer.
pub struct RawOutputCollector {
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
}

impl RawOutputCollector {
    pub fn new() -> Self {
        Self {
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
        }
    }

    pub fn stdout(&self) -> String {
        self.stdout_lines.join("\n")
    }

    pub fn stderr(&self) -> String {
        self.stderr_lines.join("\n")
    }
}

impl Default for RawOutputCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler for RawOutputCollector {
    fn handle(&mut self, event: &TestEvent) {
        if let TestEvent::RawOutput { stream, line } = event {
            match stream {
                Stream::Stdout => self.stdout_lines.push(line.clone()),
                Stream::Stderr => self.stderr_lines.push(line.clone()),
            }
        }
    }
}

/// An event handler that logs events with timestamps for debugging.
pub struct TimestampedLogger {
    start: std::time::Instant,
    entries: Vec<(Duration, String)>,
}

impl TimestampedLogger {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[(Duration, String)] {
        &self.entries
    }
}

impl Default for TimestampedLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler for TimestampedLogger {
    fn handle(&mut self, event: &TestEvent) {
        let elapsed = self.start.elapsed();
        let description = match event {
            TestEvent::RunStarted { adapter, .. } => format!("run started: {}", adapter),
            TestEvent::SuiteStarted { name } => format!("suite started: {}", name),
            TestEvent::TestStarted { suite, name } => {
                format!("test started: {}::{}", suite, name)
            }
            TestEvent::TestFinished { suite, test } => {
                format!(
                    "test finished: {}::{} ({:?})",
                    suite, test.name, test.status
                )
            }
            TestEvent::SuiteFinished { suite } => format!("suite finished: {}", suite.name),
            TestEvent::RunFinished { result } => {
                format!(
                    "run finished: {} tests, {} passed, {} failed",
                    result.total_tests(),
                    result.total_passed(),
                    result.total_failed()
                )
            }
            TestEvent::RawOutput { stream, .. } => format!("raw output ({:?})", stream),
            TestEvent::WatchRerun { changed_files } => {
                format!("watch rerun: {} files changed", changed_files.len())
            }
            TestEvent::RetryStarted {
                test_name, attempt, ..
            } => format!("retry: {} attempt {}", test_name, attempt),
            TestEvent::RetryFinished {
                test_name, passed, ..
            } => {
                format!(
                    "retry finished: {} {}",
                    test_name,
                    if *passed { "passed" } else { "failed" }
                )
            }
            TestEvent::FilterApplied {
                pattern,
                matched_count,
            } => format!("filter: '{}' matched {} tests", pattern, matched_count),
            TestEvent::ParallelAdapterStarted { adapter } => {
                format!("parallel started: {}", adapter)
            }
            TestEvent::ParallelAdapterFinished { adapter, .. } => {
                format!("parallel finished: {}", adapter)
            }
            TestEvent::Warning { message } => format!("warning: {}", message),
            TestEvent::Progress {
                message,
                current,
                total,
            } => format!("progress: {} ({}/{})", message, current, total),
        };
        self.entries.push((elapsed, description));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TestStatus;

    fn make_test_case(name: &str, status: TestStatus) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(10),
            error: None,
        }
    }

    #[test]
    fn event_bus_empty() {
        let mut bus = EventBus::new();
        assert_eq!(bus.handler_count(), 0);
        // Should not panic with no handlers
        bus.emit(TestEvent::RunStarted {
            adapter: "rust".into(),
            framework: "cargo test".into(),
            project_dir: PathBuf::from("."),
        });
    }

    #[test]
    fn event_bus_subscribe_and_emit() {
        let mut bus = EventBus::new();
        bus.subscribe(Box::new(CollectingHandler::new()));
        assert_eq!(bus.handler_count(), 1);

        bus.emit(TestEvent::RunStarted {
            adapter: "rust".into(),
            framework: "cargo test".into(),
            project_dir: PathBuf::from("."),
        });

        bus.emit(TestEvent::Warning {
            message: "something".into(),
        });
    }

    #[test]
    fn counting_handler_counts_events() {
        let mut handler = CountingHandler::default();

        handler.handle(&TestEvent::RunStarted {
            adapter: "go".into(),
            framework: "go test".into(),
            project_dir: PathBuf::from("."),
        });
        handler.handle(&TestEvent::SuiteStarted {
            name: "main".into(),
        });
        handler.handle(&TestEvent::TestStarted {
            suite: "main".into(),
            name: "TestFoo".into(),
        });
        handler.handle(&TestEvent::TestFinished {
            suite: "main".into(),
            test: make_test_case("TestFoo", TestStatus::Passed),
        });
        handler.handle(&TestEvent::SuiteFinished {
            suite: TestSuite {
                name: "main".into(),
                tests: vec![make_test_case("TestFoo", TestStatus::Passed)],
            },
        });
        handler.handle(&TestEvent::RunFinished {
            result: TestRunResult {
                suites: vec![],
                duration: Duration::from_millis(100),
                raw_exit_code: 0,
            },
        });
        handler.handle(&TestEvent::Warning {
            message: "slow".into(),
        });

        assert_eq!(handler.run_started, 1);
        assert_eq!(handler.suite_started, 1);
        assert_eq!(handler.test_started, 1);
        assert_eq!(handler.test_finished, 1);
        assert_eq!(handler.suite_finished, 1);
        assert_eq!(handler.run_finished, 1);
        assert_eq!(handler.warnings, 1);
        assert_eq!(handler.total, 7);
    }

    #[test]
    fn raw_output_collector() {
        let mut collector = RawOutputCollector::new();

        collector.handle(&TestEvent::RawOutput {
            stream: Stream::Stdout,
            line: "line 1".into(),
        });
        collector.handle(&TestEvent::RawOutput {
            stream: Stream::Stderr,
            line: "err 1".into(),
        });
        collector.handle(&TestEvent::RawOutput {
            stream: Stream::Stdout,
            line: "line 2".into(),
        });
        // Non-raw events ignored
        collector.handle(&TestEvent::Warning {
            message: "ignored".into(),
        });

        assert_eq!(collector.stdout_lines.len(), 2);
        assert_eq!(collector.stderr_lines.len(), 1);
        assert_eq!(collector.stdout(), "line 1\nline 2");
        assert_eq!(collector.stderr(), "err 1");
    }

    #[test]
    fn timestamped_logger() {
        let mut logger = TimestampedLogger::new();

        logger.handle(&TestEvent::RunStarted {
            adapter: "python".into(),
            framework: "pytest".into(),
            project_dir: PathBuf::from("."),
        });
        logger.handle(&TestEvent::Warning {
            message: "slow test".into(),
        });

        assert_eq!(logger.entries().len(), 2);
        assert!(logger.entries()[0].1.contains("run started: python"));
        assert!(logger.entries()[1].1.contains("warning: slow test"));
    }

    #[test]
    fn collecting_handler_default() {
        let handler = CollectingHandler::default();
        assert!(handler.events.is_empty());
    }

    #[test]
    fn raw_output_collector_default() {
        let collector = RawOutputCollector::default();
        assert!(collector.stdout_lines.is_empty());
        assert!(collector.stderr_lines.is_empty());
    }

    #[test]
    fn stream_equality() {
        assert_eq!(Stream::Stdout, Stream::Stdout);
        assert_eq!(Stream::Stderr, Stream::Stderr);
        assert_ne!(Stream::Stdout, Stream::Stderr);
    }

    #[test]
    fn event_bus_flush() {
        let mut bus = EventBus::new();
        bus.subscribe(Box::new(CountingHandler::default()));
        bus.flush(); // Should not panic
    }

    #[test]
    fn event_bus_multiple_handlers() {
        let mut bus = EventBus::new();
        bus.subscribe(Box::new(CountingHandler::default()));
        bus.subscribe(Box::new(CollectingHandler::new()));
        bus.subscribe(Box::new(RawOutputCollector::new()));
        assert_eq!(bus.handler_count(), 3);

        bus.emit(TestEvent::RawOutput {
            stream: Stream::Stdout,
            line: "hello".into(),
        });
    }

    #[test]
    fn timestamped_logger_all_event_types() {
        let mut logger = TimestampedLogger::new();

        logger.handle(&TestEvent::FilterApplied {
            pattern: "test_*".into(),
            matched_count: 5,
        });
        logger.handle(&TestEvent::ParallelAdapterStarted {
            adapter: "rust".into(),
        });
        logger.handle(&TestEvent::ParallelAdapterFinished {
            adapter: "rust".into(),
            result: TestRunResult {
                suites: vec![],
                duration: Duration::ZERO,
                raw_exit_code: 0,
            },
        });
        logger.handle(&TestEvent::RetryStarted {
            test_name: "test_foo".into(),
            attempt: 2,
            max_attempts: 3,
        });
        logger.handle(&TestEvent::RetryFinished {
            test_name: "test_foo".into(),
            attempt: 2,
            passed: true,
        });
        logger.handle(&TestEvent::WatchRerun {
            changed_files: vec![PathBuf::from("src/lib.rs")],
        });
        logger.handle(&TestEvent::Progress {
            message: "running".into(),
            current: 1,
            total: 10,
        });

        assert_eq!(logger.entries().len(), 7);
        assert!(logger.entries()[0].1.contains("filter"));
        assert!(logger.entries()[1].1.contains("parallel started"));
        assert!(logger.entries()[2].1.contains("parallel finished"));
        assert!(logger.entries()[3].1.contains("retry: test_foo"));
        assert!(logger.entries()[4].1.contains("retry finished"));
        assert!(logger.entries()[5].1.contains("watch rerun"));
        assert!(logger.entries()[6].1.contains("progress"));
    }
}
