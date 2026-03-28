use std::fmt;
use std::path::PathBuf;

/// All error types that testx can produce.
#[derive(Debug)]
pub enum TestxError {
    /// No test framework could be detected in the project directory.
    NoFrameworkDetected { path: PathBuf },

    /// A required test runner binary is not on PATH.
    RunnerNotFound { runner: String },

    /// Failed to spawn or execute the test command.
    ExecutionFailed {
        command: String,
        source: std::io::Error,
    },

    /// Test process exceeded the configured timeout.
    Timeout { seconds: u64 },

    /// Could not parse test runner output into structured results.
    ParseError { message: String },

    /// Configuration file is invalid or contains bad values.
    ConfigError { message: String },

    /// The user specified an adapter that doesn't exist.
    AdapterNotFound { name: String },

    /// File system operation failed.
    IoError {
        context: String,
        source: std::io::Error,
    },

    /// Path resolution failed.
    PathError { message: String },

    /// Watch mode error (file watcher failure).
    WatchError { message: String },

    /// Plugin loading or execution error.
    PluginError { message: String },

    /// Filter pattern is invalid.
    FilterError { pattern: String, message: String },

    /// History/database error.
    HistoryError { message: String },

    /// Coverage tool error.
    CoverageError { message: String },

    /// Multiple errors collected from parallel execution.
    MultipleErrors { errors: Vec<TestxError> },
}

impl fmt::Display for TestxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestxError::NoFrameworkDetected { path } => {
                write!(
                    f,
                    "No test framework detected in '{}'. Try 'testx detect' to diagnose, \
                     or 'testx list' for supported frameworks.",
                    path.display()
                )
            }
            TestxError::RunnerNotFound { runner } => {
                write!(
                    f,
                    "Test runner '{}' not found. Install it and try again.",
                    runner
                )
            }
            TestxError::ExecutionFailed { command, source } => {
                write!(f, "Failed to execute command '{}': {}", command, source)
            }
            TestxError::Timeout { seconds } => {
                write!(f, "Test process timed out after {}s", seconds)
            }
            TestxError::ParseError { message } => {
                write!(f, "Failed to parse test output: {}", message)
            }
            TestxError::ConfigError { message } => {
                write!(f, "Configuration error: {}", message)
            }
            TestxError::AdapterNotFound { name } => {
                write!(
                    f,
                    "Adapter '{}' not found. Run 'testx list' to see available adapters.",
                    name
                )
            }
            TestxError::IoError { context, source } => {
                write!(f, "{}: {}", context, source)
            }
            TestxError::PathError { message } => {
                write!(f, "Path error: {}", message)
            }
            TestxError::WatchError { message } => {
                write!(f, "Watch error: {}", message)
            }
            TestxError::PluginError { message } => {
                write!(f, "Plugin error: {}", message)
            }
            TestxError::FilterError { pattern, message } => {
                write!(f, "Invalid filter pattern '{}': {}", pattern, message)
            }
            TestxError::HistoryError { message } => {
                write!(f, "History error: {}", message)
            }
            TestxError::CoverageError { message } => {
                write!(f, "Coverage error: {}", message)
            }
            TestxError::MultipleErrors { errors } => {
                write!(f, "Multiple errors occurred:")?;
                for (i, err) in errors.iter().enumerate() {
                    write!(f, "\n  {}. {}", i + 1, err)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for TestxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TestxError::ExecutionFailed { source, .. } => Some(source),
            TestxError::IoError { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TestxError {
    fn from(err: std::io::Error) -> Self {
        TestxError::IoError {
            context: "I/O operation failed".into(),
            source: err,
        }
    }
}

/// Convenience type alias for testx results.
pub type Result<T> = std::result::Result<T, TestxError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_no_framework() {
        let err = TestxError::NoFrameworkDetected {
            path: PathBuf::from("/tmp/project"),
        };
        let msg = err.to_string();
        assert!(msg.contains("No test framework detected"));
        assert!(msg.contains("/tmp/project"));
    }

    #[test]
    fn error_display_runner_not_found() {
        let err = TestxError::RunnerNotFound {
            runner: "cargo".into(),
        };
        assert!(err.to_string().contains("cargo"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn error_display_timeout() {
        let err = TestxError::Timeout { seconds: 30 };
        assert!(err.to_string().contains("30s"));
    }

    #[test]
    fn error_display_adapter_not_found() {
        let err = TestxError::AdapterNotFound {
            name: "haskell".into(),
        };
        assert!(err.to_string().contains("haskell"));
    }

    #[test]
    fn error_display_filter_error() {
        let err = TestxError::FilterError {
            pattern: "[invalid".into(),
            message: "unclosed bracket".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[invalid"));
        assert!(msg.contains("unclosed bracket"));
    }

    #[test]
    fn error_display_multiple_errors() {
        let err = TestxError::MultipleErrors {
            errors: vec![
                TestxError::Timeout { seconds: 10 },
                TestxError::RunnerNotFound {
                    runner: "npm".into(),
                },
            ],
        };
        let msg = err.to_string();
        assert!(msg.contains("Multiple errors"));
        assert!(msg.contains("10s"));
        assert!(msg.contains("npm"));
    }

    #[test]
    fn error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let testx_err: TestxError = io_err.into();
        assert!(testx_err.to_string().contains("file not found"));
    }

    #[test]
    fn error_display_execution_failed() {
        let err = TestxError::ExecutionFailed {
            command: "cargo test".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        assert!(err.to_string().contains("cargo test"));
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn error_display_config_error() {
        let err = TestxError::ConfigError {
            message: "invalid TOML".into(),
        };
        assert!(err.to_string().contains("invalid TOML"));
    }

    #[test]
    fn error_display_parse_error() {
        let err = TestxError::ParseError {
            message: "unexpected token".into(),
        };
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn error_display_watch_error() {
        let err = TestxError::WatchError {
            message: "inotify limit".into(),
        };
        assert!(err.to_string().contains("inotify limit"));
    }

    #[test]
    fn error_display_plugin_error() {
        let err = TestxError::PluginError {
            message: "script failed".into(),
        };
        assert!(err.to_string().contains("script failed"));
    }

    #[test]
    fn error_display_history_error() {
        let err = TestxError::HistoryError {
            message: "db locked".into(),
        };
        assert!(err.to_string().contains("db locked"));
    }

    #[test]
    fn error_display_coverage_error() {
        let err = TestxError::CoverageError {
            message: "lcov not found".into(),
        };
        assert!(err.to_string().contains("lcov not found"));
    }

    #[test]
    fn error_display_path_error() {
        let err = TestxError::PathError {
            message: "not absolute".into(),
        };
        assert!(err.to_string().contains("not absolute"));
    }

    #[test]
    fn error_display_io_error() {
        let err = TestxError::IoError {
            context: "reading config".into(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
        };
        let msg = err.to_string();
        assert!(msg.contains("reading config"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn error_source_chain() {
        let err = TestxError::ExecutionFailed {
            command: "test".into(),
            source: std::io::Error::other("boom"),
        };
        assert!(std::error::Error::source(&err).is_some());

        let err2 = TestxError::Timeout { seconds: 5 };
        assert!(std::error::Error::source(&err2).is_none());
    }
}
