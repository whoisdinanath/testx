use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::adapters::TestRunResult;
use crate::config::Config;
use crate::detection::DetectionEngine;
use crate::error::{Result, TestxError};
use crate::events::{EventBus, Stream, TestEvent};

/// Configuration for a test run.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Project directory to run tests in.
    pub project_dir: PathBuf,

    /// Override adapter selection by name.
    pub adapter_override: Option<String>,

    /// Extra arguments to pass to the test runner.
    pub extra_args: Vec<String>,

    /// Maximum time to wait for test completion.
    pub timeout: Option<Duration>,

    /// Environment variables to set for the test process.
    pub env: HashMap<String, String>,

    /// Number of times to retry failed tests.
    pub retries: u32,

    /// Stop on first test failure.
    pub fail_fast: bool,

    /// Test name filter pattern.
    pub filter: Option<String>,

    /// Exclude pattern.
    pub exclude: Option<String>,

    /// Verbose mode (print commands, detection details).
    pub verbose: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            project_dir: PathBuf::from("."),
            adapter_override: None,
            extra_args: Vec::new(),
            timeout: None,
            env: HashMap::new(),
            retries: 0,
            fail_fast: false,
            filter: None,
            exclude: None,
            verbose: false,
        }
    }
}

impl RunnerConfig {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            project_dir,
            ..Default::default()
        }
    }

    /// Merge values from a Config file (CLI args take precedence).
    pub fn merge_config(&mut self, config: &Config) {
        if self.adapter_override.is_none() {
            self.adapter_override = config.adapter.clone();
        }
        if self.extra_args.is_empty() {
            self.extra_args = config.args.clone();
        }
        if self.timeout.is_none() {
            self.timeout = config.timeout.map(Duration::from_secs);
        }
        for (key, value) in &config.env {
            self.env.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

/// Execution result with raw output captured for display purposes.
#[derive(Debug, Clone)]
pub struct ExecutionOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub timed_out: bool,
}

/// The main test runner engine.
pub struct Runner {
    engine: DetectionEngine,
    config: RunnerConfig,
    event_bus: EventBus,
}

impl Runner {
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            engine: DetectionEngine::new(),
            config,
            event_bus: EventBus::new(),
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = event_bus;
        self
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn event_bus_mut(&mut self) -> &mut EventBus {
        &mut self.event_bus
    }

    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    pub fn engine(&self) -> &DetectionEngine {
        &self.engine
    }

    /// Run tests, auto-detecting the adapter or using the configured override.
    pub fn run(&mut self) -> Result<(TestRunResult, ExecutionOutput)> {
        let (adapter_index, adapter_name, framework) = self.resolve_adapter()?;

        self.event_bus.emit(TestEvent::RunStarted {
            adapter: adapter_name.clone(),
            framework: framework.clone(),
            project_dir: self.config.project_dir.clone(),
        });

        // Phase 1: borrow engine immutably to build command and check runner
        let (mut cmd, _adapter_name_check) = {
            let adapter = self.engine.adapter(adapter_index);

            if let Some(missing) = adapter.check_runner() {
                return Err(TestxError::RunnerNotFound { runner: missing });
            }

            let cmd = adapter
                .build_command(&self.config.project_dir, &self.config.extra_args)
                .map_err(|e| TestxError::ExecutionFailed {
                    command: adapter_name.clone(),
                    source: std::io::Error::other(e.to_string()),
                })?;

            (cmd, adapter.name().to_string())
        };

        // Set environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        if self.config.verbose {
            eprintln!("cmd: {:?}", cmd);
        }

        // Phase 2: execute (borrows self mutably for event bus)
        let exec_output = self.execute_command(&mut cmd)?;

        // Phase 3: parse (borrows engine immutably again)
        let adapter = self.engine.adapter(adapter_index);
        let mut result =
            adapter.parse_output(&exec_output.stdout, &exec_output.stderr, exec_output.exit_code);

        // Use wall-clock time if parser didn't capture duration
        if result.duration.as_millis() == 0 {
            result.duration = exec_output.duration;
        }

        self.event_bus.emit(TestEvent::RunFinished {
            result: result.clone(),
        });
        self.event_bus.flush();

        Ok((result, exec_output))
    }

    /// Run tests using a specific adapter by index.
    pub fn run_with_adapter(
        &mut self,
        adapter_index: usize,
    ) -> Result<(TestRunResult, ExecutionOutput)> {
        // Phase 1: borrow engine to build command
        let (mut cmd, adapter_name) = {
            let adapter = self.engine.adapter(adapter_index);
            let name = adapter.name().to_string();

            if let Some(missing) = adapter.check_runner() {
                return Err(TestxError::RunnerNotFound { runner: missing });
            }

            let cmd = adapter
                .build_command(&self.config.project_dir, &self.config.extra_args)
                .map_err(|e| TestxError::ExecutionFailed {
                    command: name.clone(),
                    source: std::io::Error::other(e.to_string()),
                })?;

            (cmd, name)
        };

        self.event_bus.emit(TestEvent::RunStarted {
            adapter: adapter_name.clone(),
            framework: adapter_name.clone(),
            project_dir: self.config.project_dir.clone(),
        });

        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        // Phase 2: execute
        let exec_output = self.execute_command(&mut cmd)?;

        // Phase 3: parse
        let adapter = self.engine.adapter(adapter_index);
        let mut result =
            adapter.parse_output(&exec_output.stdout, &exec_output.stderr, exec_output.exit_code);

        if result.duration.as_millis() == 0 {
            result.duration = exec_output.duration;
        }

        self.event_bus.emit(TestEvent::RunFinished {
            result: result.clone(),
        });
        self.event_bus.flush();

        Ok((result, exec_output))
    }

    /// Resolve which adapter to use: explicit override or auto-detect.
    fn resolve_adapter(&self) -> Result<(usize, String, String)> {
        if let Some(name) = &self.config.adapter_override {
            let index = self
                .engine
                .adapters()
                .iter()
                .position(|a| a.name().to_lowercase() == name.to_lowercase())
                .ok_or_else(|| TestxError::AdapterNotFound { name: name.clone() })?;

            let adapter = self.engine.adapter(index);
            Ok((index, adapter.name().to_string(), adapter.name().to_string()))
        } else {
            let detected = self
                .engine
                .detect(&self.config.project_dir)
                .ok_or_else(|| TestxError::NoFrameworkDetected {
                    path: self.config.project_dir.clone(),
                })?;

            let adapter = self.engine.adapter(detected.adapter_index);
            Ok((
                detected.adapter_index,
                adapter.name().to_string(),
                detected.detection.framework.clone(),
            ))
        }
    }

    /// Execute a command, streaming output line-by-line and respecting timeouts.
    fn execute_command(&mut self, cmd: &mut Command) -> Result<ExecutionOutput> {
        let start = Instant::now();

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TestxError::ExecutionFailed {
                command: format!("{:?}", cmd),
                source: e,
            })?;

        // Take ownership of stdout/stderr pipes
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        // Channel for collecting lines from both streams
        let (tx, rx) = mpsc::channel();

        // Spawn stdout reader thread
        let tx_out = tx.clone();
        let stdout_handle = thread::spawn(move || {
            let mut lines = Vec::new();
            if let Some(pipe) = child_stdout {
                let reader = BufReader::new(pipe);
                for line in reader.lines().map_while(|r| r.ok()) {
                    let _ = tx_out.send((Stream::Stdout, line.clone()));
                    lines.push(line);
                }
            }
            lines
        });

        // Spawn stderr reader thread
        let stderr_handle = thread::spawn(move || {
            let mut lines = Vec::new();
            if let Some(pipe) = child_stderr {
                let reader = BufReader::new(pipe);
                for line in reader.lines().map_while(|r| r.ok()) {
                    let _ = tx.send((Stream::Stderr, line.clone()));
                    lines.push(line);
                }
            }
            lines
        });

        // Process events from stream readers
        let timeout = self.config.timeout;
        let mut timed_out = false;

        // Drop rx in a non-blocking way if we have a timeout
        if let Some(timeout_dur) = timeout {
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((stream, line)) => {
                        self.event_bus.emit(TestEvent::RawOutput { stream, line });
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if start.elapsed() > timeout_dur {
                            timed_out = true;
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        } else {
            // No timeout — just drain events
            for (stream, line) in rx {
                self.event_bus.emit(TestEvent::RawOutput { stream, line });
            }
        }

        // Collect results from threads
        let stdout_lines = stdout_handle.join().unwrap_or_default();
        let stderr_lines = stderr_handle.join().unwrap_or_default();

        let exit_code = if timed_out {
            124
        } else {
            child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
        };

        let duration = start.elapsed();

        if timed_out
            && let Some(secs) = self.config.timeout {
                self.event_bus.emit(TestEvent::Warning {
                    message: format!("Test timed out after {}s", secs.as_secs()),
                });
            }

        Ok(ExecutionOutput {
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
            exit_code,
            duration,
            timed_out,
        })
    }
}

/// Build a RunnerConfig from CLI args and config file.
pub fn build_runner_config(
    project_dir: PathBuf,
    config: &Config,
    extra_args: Vec<String>,
    timeout: Option<u64>,
    verbose: bool,
) -> RunnerConfig {
    let mut rc = RunnerConfig::new(project_dir);
    rc.extra_args = extra_args;
    rc.timeout = timeout.map(Duration::from_secs);
    rc.verbose = verbose;
    rc.merge_config(config);
    rc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_config_default() {
        let cfg = RunnerConfig::default();
        assert_eq!(cfg.project_dir, PathBuf::from("."));
        assert!(cfg.adapter_override.is_none());
        assert!(cfg.extra_args.is_empty());
        assert!(cfg.timeout.is_none());
        assert!(cfg.env.is_empty());
        assert_eq!(cfg.retries, 0);
        assert!(!cfg.fail_fast);
        assert!(cfg.filter.is_none());
        assert!(cfg.exclude.is_none());
        assert!(!cfg.verbose);
    }

    #[test]
    fn runner_config_new() {
        let cfg = RunnerConfig::new(PathBuf::from("/tmp/project"));
        assert_eq!(cfg.project_dir, PathBuf::from("/tmp/project"));
    }

    #[test]
    fn runner_config_merge_config() {
        let mut rc = RunnerConfig::new(PathBuf::from("."));

        let config = Config {
            adapter: Some("python".into()),
            args: vec!["-v".into()],
            timeout: Some(60),
            env: HashMap::from([("CI".into(), "true".into())]),
            ..Default::default()
        };

        rc.merge_config(&config);

        assert_eq!(rc.adapter_override.as_deref(), Some("python"));
        assert_eq!(rc.extra_args, vec!["-v"]);
        assert_eq!(rc.timeout, Some(Duration::from_secs(60)));
        assert_eq!(rc.env.get("CI").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn runner_config_merge_cli_takes_precedence() {
        let mut rc = RunnerConfig::new(PathBuf::from("."));
        rc.adapter_override = Some("rust".into());
        rc.extra_args = vec!["--release".into()];
        rc.timeout = Some(Duration::from_secs(30));
        rc.env.insert("CI".into(), "false".into());

        let config = Config {
            adapter: Some("python".into()),
            args: vec!["-v".into()],
            timeout: Some(60),
            env: HashMap::from([("CI".into(), "true".into())]),
            ..Default::default()
        };

        rc.merge_config(&config);

        // CLI values should win
        assert_eq!(rc.adapter_override.as_deref(), Some("rust"));
        assert_eq!(rc.extra_args, vec!["--release"]);
        assert_eq!(rc.timeout, Some(Duration::from_secs(30)));
        assert_eq!(rc.env.get("CI").map(|s| s.as_str()), Some("false"));
    }

    #[test]
    fn build_runner_config_function() {
        let mut config = Config::default();
        config.env.insert("FOO".into(), "bar".into());

        let rc = build_runner_config(
            PathBuf::from("/tmp"),
            &config,
            vec!["--arg".into()],
            Some(30),
            true,
        );

        assert_eq!(rc.project_dir, PathBuf::from("/tmp"));
        assert_eq!(rc.extra_args, vec!["--arg"]);
        assert_eq!(rc.timeout, Some(Duration::from_secs(30)));
        assert!(rc.verbose);
        assert_eq!(rc.env.get("FOO").map(|s| s.as_str()), Some("bar"));
    }

    #[test]
    fn runner_new() {
        let cfg = RunnerConfig::new(PathBuf::from("."));
        let runner = Runner::new(cfg);
        assert_eq!(runner.config().project_dir, PathBuf::from("."));
        assert_eq!(runner.event_bus().handler_count(), 0);
    }

    #[test]
    fn runner_with_event_bus() {
        use crate::events::CountingHandler;

        let cfg = RunnerConfig::new(PathBuf::from("."));
        let mut bus = EventBus::new();
        bus.subscribe(Box::new(CountingHandler::default()));

        let runner = Runner::new(cfg).with_event_bus(bus);
        assert_eq!(runner.event_bus().handler_count(), 1);
    }

    #[test]
    fn runner_resolve_adapter_not_found() {
        let mut cfg = RunnerConfig::new(PathBuf::from("."));
        cfg.adapter_override = Some("nonexistent_language".into());

        let runner = Runner::new(cfg);
        let result = runner.resolve_adapter();
        assert!(result.is_err());

        match result.unwrap_err() {
            TestxError::AdapterNotFound { name } => {
                assert_eq!(name, "nonexistent_language");
            }
            other => panic!("expected AdapterNotFound, got: {}", other),
        }
    }

    #[test]
    fn runner_resolve_adapter_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = RunnerConfig::new(dir.path().to_path_buf());
        cfg.adapter_override = Some("Rust".into());

        let runner = Runner::new(cfg);
        let (index, name, _) = runner.resolve_adapter().unwrap();
        assert_eq!(name, "Rust");
        assert!(index < runner.engine().adapters().len());
    }

    #[test]
    fn runner_resolve_adapter_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = RunnerConfig::new(dir.path().to_path_buf());
        cfg.adapter_override = Some("python".into());

        let runner = Runner::new(cfg);
        let (_, name, _) = runner.resolve_adapter().unwrap();
        assert_eq!(name, "Python");
    }

    #[test]
    fn runner_resolve_adapter_auto_detect() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let cfg = RunnerConfig::new(dir.path().to_path_buf());
        let runner = Runner::new(cfg);
        let (_, name, framework) = runner.resolve_adapter().unwrap();
        assert_eq!(name, "Rust");
        assert_eq!(framework, "cargo test");
    }

    #[test]
    fn runner_resolve_adapter_no_framework() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunnerConfig::new(dir.path().to_path_buf());
        let runner = Runner::new(cfg);
        let result = runner.resolve_adapter();
        assert!(result.is_err());

        match result.unwrap_err() {
            TestxError::NoFrameworkDetected { path } => {
                assert_eq!(path, dir.path().to_path_buf());
            }
            other => panic!("expected NoFrameworkDetected, got: {}", other),
        }
    }

    #[test]
    fn execution_output_fields() {
        let output = ExecutionOutput {
            stdout: "hello".into(),
            stderr: "world".into(),
            exit_code: 0,
            duration: Duration::from_millis(100),
            timed_out: false,
        };

        assert_eq!(output.stdout, "hello");
        assert_eq!(output.stderr, "world");
        assert_eq!(output.exit_code, 0);
        assert!(!output.timed_out);
    }

    #[test]
    fn execution_output_timed_out() {
        let output = ExecutionOutput {
            stdout: String::new(),
            stderr: "Timed out".into(),
            exit_code: 124,
            duration: Duration::from_secs(30),
            timed_out: true,
        };

        assert!(output.timed_out);
        assert_eq!(output.exit_code, 124);
    }
}
