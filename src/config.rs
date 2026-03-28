use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Configuration loaded from `testx.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Override adapter selection (e.g. "python", "rust", "java")
    pub adapter: Option<String>,

    /// Extra arguments to pass to the test runner
    pub args: Vec<String>,

    /// Timeout in seconds (0 = no timeout)
    pub timeout: Option<u64>,

    /// Stop on first failure
    pub fail_fast: Option<bool>,

    /// Number of retries for failed tests
    pub retries: Option<u32>,

    /// Run all detected adapters in parallel
    pub parallel: Option<bool>,

    /// Environment variables to set before running tests
    pub env: HashMap<String, String>,

    /// Filtering configuration
    pub filter: Option<FilterConfig>,

    /// Watch mode configuration
    pub watch: Option<WatchConfig>,

    /// Output configuration
    pub output: Option<OutputConfig>,

    /// Per-adapter configuration overrides
    pub adapters: Option<HashMap<String, AdapterConfig>>,

    /// Custom adapter definitions
    pub custom_adapter: Option<Vec<CustomAdapterConfig>>,

    /// Coverage configuration
    pub coverage: Option<CoverageConfig>,

    /// History/analytics configuration
    pub history: Option<HistoryConfig>,
}

/// Filter configuration section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FilterConfig {
    /// Include pattern (glob or regex)
    pub include: Option<String>,
    /// Exclude pattern (glob or regex)
    pub exclude: Option<String>,
}

/// Watch mode configuration section.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    /// Enable watch mode by default
    pub enabled: bool,
    /// Clear screen between runs
    pub clear: bool,
    /// Debounce time in milliseconds
    pub debounce_ms: u64,
    /// Patterns to ignore
    pub ignore: Vec<String>,
    /// Poll interval for network filesystems (ms, 0 = use native events)
    pub poll_ms: Option<u64>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            clear: true,
            debounce_ms: 300,
            ignore: vec![
                "*.pyc".into(),
                "__pycache__".into(),
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                ".testx".into(),
            ],
            poll_ms: None,
        }
    }
}

/// Output configuration section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Default output format
    pub format: Option<String>,
    /// Show N slowest tests
    pub slowest: Option<usize>,
    /// Verbose mode
    pub verbose: Option<bool>,
    /// Color mode: auto, always, never
    pub colors: Option<String>,
}

/// Per-adapter configuration overrides.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AdapterConfig {
    /// Override runner (e.g., "pytest" vs "unittest")
    pub runner: Option<String>,
    /// Extra arguments for this specific adapter
    pub args: Vec<String>,
    /// Environment variables specific to this adapter
    pub env: HashMap<String, String>,
    /// Timeout override for this adapter
    pub timeout: Option<u64>,
}

/// Custom adapter definition.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomAdapterConfig {
    /// Name for the custom adapter
    pub name: String,
    /// File whose presence triggers detection
    pub detect: String,
    /// Command to run
    pub command: String,
    /// Default arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Output parser: "json", "junit", "tap", "lines"
    #[serde(default = "default_parser")]
    pub parse: String,
    /// Detection confidence (0.0 to 1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_parser() -> String {
    "lines".into()
}

fn default_confidence() -> f32 {
    0.5
}

/// Coverage configuration section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CoverageConfig {
    /// Enable coverage collection
    pub enabled: bool,
    /// Output format: "summary", "lcov", "html", "cobertura"
    pub format: Option<String>,
    /// Output directory for coverage reports
    pub output_dir: Option<String>,
    /// Minimum coverage threshold (fail below this)
    pub threshold: Option<f64>,
}

/// History/analytics configuration section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Enable history recording
    pub enabled: bool,
    /// Maximum age of history entries in days
    pub max_age_days: Option<u32>,
    /// Database path (default: .testx/history.db)
    pub db_path: Option<String>,
}

impl Config {
    /// Load config from `testx.toml` in the given directory.
    /// Returns `Config::default()` if no config file exists.
    pub fn load(project_dir: &Path) -> Self {
        let config_path = project_dir.join("testx.toml");
        if !config_path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: failed to parse testx.toml: {e}");
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read testx.toml: {e}");
                Self::default()
            }
        }
    }

    /// Get adapter-specific config if available.
    pub fn adapter_config(&self, adapter_name: &str) -> Option<&AdapterConfig> {
        self.adapters
            .as_ref()
            .and_then(|m| m.get(&adapter_name.to_lowercase()))
    }

    /// Get watch config, or default.
    pub fn watch_config(&self) -> WatchConfig {
        self.watch.clone().unwrap_or_default()
    }

    /// Get output config, or default.
    pub fn output_config(&self) -> OutputConfig {
        self.output.clone().unwrap_or_default()
    }

    /// Get filter config, or default.
    pub fn filter_config(&self) -> FilterConfig {
        self.filter.clone().unwrap_or_default()
    }

    /// Get coverage config, or default.
    pub fn coverage_config(&self) -> CoverageConfig {
        self.coverage.clone().unwrap_or_default()
    }

    /// Get history config, or default.
    pub fn history_config(&self) -> HistoryConfig {
        self.history.clone().unwrap_or_default()
    }

    /// Check if watch mode is enabled (via config or CLI).
    pub fn is_watch_enabled(&self) -> bool {
        self.watch.as_ref().map(|w| w.enabled).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path());
        assert!(config.adapter.is_none());
        assert!(config.args.is_empty());
        assert!(config.timeout.is_none());
        assert!(config.env.is_empty());
    }

    #[test]
    fn load_minimal_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"adapter = "python"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        assert_eq!(config.adapter.as_deref(), Some("python"));
    }

    #[test]
    fn load_full_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
adapter = "rust"
args = ["--release", "--", "--nocapture"]
timeout = 60

[env]
RUST_LOG = "debug"
CI = "true"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        assert_eq!(config.adapter.as_deref(), Some("rust"));
        assert_eq!(config.args, vec!["--release", "--", "--nocapture"]);
        assert_eq!(config.timeout, Some(60));
        assert_eq!(
            config.env.get("RUST_LOG").map(|s| s.as_str()),
            Some("debug")
        );
        assert_eq!(config.env.get("CI").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn load_invalid_config_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("testx.toml"), "this is not valid toml {{{}").unwrap();
        let config = Config::load(dir.path());
        assert!(config.adapter.is_none());
    }

    #[test]
    fn load_config_with_only_args() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"args = ["-v", "--no-header"]"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        assert!(config.adapter.is_none());
        assert_eq!(config.args.len(), 2);
    }

    #[test]
    fn load_config_with_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[filter]
include = "test_auth*"
exclude = "test_slow*"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let filter = config.filter_config();
        assert_eq!(filter.include.as_deref(), Some("test_auth*"));
        assert_eq!(filter.exclude.as_deref(), Some("test_slow*"));
    }

    #[test]
    fn load_config_with_watch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[watch]
enabled = true
clear = false
debounce_ms = 500
ignore = ["*.pyc", ".git"]
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        assert!(config.is_watch_enabled());
        let watch = config.watch_config();
        assert!(!watch.clear);
        assert_eq!(watch.debounce_ms, 500);
        assert_eq!(watch.ignore.len(), 2);
    }

    #[test]
    fn load_config_with_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[output]
format = "json"
slowest = 5
verbose = true
colors = "never"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let output = config.output_config();
        assert_eq!(output.format.as_deref(), Some("json"));
        assert_eq!(output.slowest, Some(5));
        assert_eq!(output.verbose, Some(true));
        assert_eq!(output.colors.as_deref(), Some("never"));
    }

    #[test]
    fn load_config_with_adapter_overrides() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[adapters.python]
runner = "pytest"
args = ["-x", "--tb=short"]
timeout = 120

[adapters.javascript]
runner = "vitest"
args = ["--reporter=verbose"]
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let py = config.adapter_config("python").unwrap();
        assert_eq!(py.runner.as_deref(), Some("pytest"));
        assert_eq!(py.args, vec!["-x", "--tb=short"]);
        assert_eq!(py.timeout, Some(120));

        let js = config.adapter_config("javascript").unwrap();
        assert_eq!(js.runner.as_deref(), Some("vitest"));
    }

    #[test]
    fn load_config_with_custom_adapter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[[custom_adapter]]
name = "bazel"
detect = "BUILD"
command = "bazel test //..."
args = ["--test_output=all"]
parse = "tap"
confidence = 0.7
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let custom = config.custom_adapter.as_ref().unwrap();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].name, "bazel");
        assert_eq!(custom[0].detect, "BUILD");
        assert_eq!(custom[0].command, "bazel test //...");
        assert_eq!(custom[0].parse, "tap");
        assert!((custom[0].confidence - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn load_config_with_coverage() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[coverage]
enabled = true
format = "lcov"
threshold = 80.0
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let cov = config.coverage_config();
        assert!(cov.enabled);
        assert_eq!(cov.format.as_deref(), Some("lcov"));
        assert_eq!(cov.threshold, Some(80.0));
    }

    #[test]
    fn load_config_with_history() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[history]
enabled = true
max_age_days = 90
db_path = ".testx/data.db"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        let hist = config.history_config();
        assert!(hist.enabled);
        assert_eq!(hist.max_age_days, Some(90));
        assert_eq!(hist.db_path.as_deref(), Some(".testx/data.db"));
    }

    #[test]
    fn load_config_fail_fast_and_retries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
fail_fast = true
retries = 3
parallel = true
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        assert_eq!(config.fail_fast, Some(true));
        assert_eq!(config.retries, Some(3));
        assert_eq!(config.parallel, Some(true));
    }

    #[test]
    fn default_watch_config() {
        let watch = WatchConfig::default();
        assert!(!watch.enabled);
        assert!(watch.clear);
        assert_eq!(watch.debounce_ms, 300);
        assert!(watch.ignore.contains(&".git".to_string()));
        assert!(watch.ignore.contains(&"node_modules".to_string()));
    }

    #[test]
    fn adapter_config_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("testx.toml"),
            r#"
[adapters.python]
runner = "pytest"
"#,
        )
        .unwrap();
        let config = Config::load(dir.path());
        // adapter_config lowercases the input, so both work
        assert!(config.adapter_config("Python").is_some());
        assert!(config.adapter_config("python").is_some());
    }

    #[test]
    fn watch_not_enabled_by_default() {
        let config = Config::default();
        assert!(!config.is_watch_enabled());
    }

    #[test]
    fn default_configs_return_defaults() {
        let config = Config::default();
        let _ = config.filter_config();
        let _ = config.output_config();
        let _ = config.coverage_config();
        let _ = config.history_config();
        let _ = config.watch_config();
    }
}
