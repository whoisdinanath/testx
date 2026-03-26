//! Plugin system for testx.
//!
//! Plugins receive events during test execution and can produce custom output,
//! send notifications, or perform post-processing on test results.

pub mod reporters;
pub mod script_adapter;

use crate::adapters::TestRunResult;
use crate::error;
use crate::events::TestEvent;

/// A plugin that hooks into the test execution lifecycle.
///
/// Plugins receive events as they occur and can act on the final result.
/// They are registered with a PluginManager before the test run starts.
pub trait Plugin: Send {
    /// Unique name identifying this plugin.
    fn name(&self) -> &str;

    /// Plugin version string.
    fn version(&self) -> &str;

    /// Called for each event during test execution.
    fn on_event(&mut self, event: &TestEvent) -> error::Result<()>;

    /// Called once the test run is complete with the final result.
    fn on_result(&mut self, result: &TestRunResult) -> error::Result<()>;

    /// Called when the plugin is being shut down (cleanup).
    fn shutdown(&mut self) -> error::Result<()> {
        Ok(())
    }
}

/// Manages a collection of plugins and dispatches events to them.
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    errors: Vec<PluginError>,
}

/// An error that occurred in a specific plugin.
#[derive(Debug, Clone)]
pub struct PluginError {
    /// Name of the plugin that errored.
    pub plugin_name: String,
    /// Error message.
    pub message: String,
    /// Whether the error is fatal (should abort the run).
    pub fatal: bool,
}

impl PluginError {
    fn new(plugin_name: &str, message: String, fatal: bool) -> Self {
        Self {
            plugin_name: plugin_name.to_string(),
            message,
            fatal,
        }
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[plugin:{}] {}{}",
            self.plugin_name,
            self.message,
            if self.fatal { " (fatal)" } else { "" }
        )
    }
}

impl PluginManager {
    /// Create a new empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Register a plugin.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// Number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Get names of all registered plugins.
    pub fn plugin_names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }

    /// Whether any plugin has reported a fatal error.
    pub fn has_fatal_error(&self) -> bool {
        self.errors.iter().any(|e| e.fatal)
    }

    /// Get all plugin errors that occurred.
    pub fn errors(&self) -> &[PluginError] {
        &self.errors
    }

    /// Clear collected errors.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Dispatch an event to all registered plugins.
    ///
    /// Errors are collected but do not stop dispatch to other plugins.
    pub fn dispatch_event(&mut self, event: &TestEvent) {
        for plugin in &mut self.plugins {
            if let Err(e) = plugin.on_event(event) {
                self.errors.push(PluginError::new(
                    plugin.name(),
                    format!("on_event error: {e}"),
                    false,
                ));
            }
        }
    }

    /// Dispatch the final result to all registered plugins.
    pub fn dispatch_result(&mut self, result: &TestRunResult) {
        for plugin in &mut self.plugins {
            if let Err(e) = plugin.on_result(result) {
                self.errors.push(PluginError::new(
                    plugin.name(),
                    format!("on_result error: {e}"),
                    false,
                ));
            }
        }
    }

    /// Shut down all plugins, collecting any errors.
    pub fn shutdown_all(&mut self) {
        for plugin in &mut self.plugins {
            if let Err(e) = plugin.shutdown() {
                self.errors.push(PluginError::new(
                    plugin.name(),
                    format!("shutdown error: {e}"),
                    false,
                ));
            }
        }
    }

    /// Remove a plugin by name. Returns true if found and removed.
    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.plugins.len();
        self.plugins.retain(|p| p.name() != name);
        self.plugins.len() < len_before
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin metadata exposed for listing/discovery.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

impl PluginInfo {
    pub fn new(name: &str, version: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
        }
    }
}

/// Registry of available plugin types for discovery.
pub struct PluginRegistry {
    available: Vec<PluginInfo>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            available: Vec::new(),
        }
    }

    /// Register an available plugin type.
    pub fn register_available(&mut self, info: PluginInfo) {
        self.available.push(info);
    }

    /// Get all available plugin types.
    pub fn list_available(&self) -> &[PluginInfo] {
        &self.available
    }

    /// Build the default registry with all built-in plugins.
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register_available(PluginInfo::new(
            "markdown",
            "1.0.0",
            "Generates a Markdown test report",
        ));
        registry.register_available(PluginInfo::new(
            "github",
            "1.0.0",
            "Emits GitHub Actions annotations",
        ));
        registry.register_available(PluginInfo::new(
            "html",
            "1.0.0",
            "Generates a self-contained HTML test report",
        ));
        registry.register_available(PluginInfo::new(
            "notify",
            "1.0.0",
            "Sends desktop notifications on test completion",
        ));
        registry
    }

    /// Find a plugin by name.
    pub fn find(&self, name: &str) -> Option<&PluginInfo> {
        self.available.iter().find(|p| p.name == name)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};
    use std::time::Duration;

    /// A test plugin for unit testing.
    struct MockPlugin {
        name: String,
        events_received: Vec<String>,
        result_received: bool,
        shutdown_called: bool,
        should_error: bool,
    }

    impl MockPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                events_received: Vec::new(),
                result_received: false,
                shutdown_called: false,
                should_error: false,
            }
        }

        fn failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                events_received: Vec::new(),
                result_received: false,
                shutdown_called: false,
                should_error: true,
            }
        }
    }

    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "0.1.0"
        }

        fn on_event(&mut self, event: &TestEvent) -> error::Result<()> {
            if self.should_error {
                return Err(error::TestxError::PluginError {
                    message: "mock error".into(),
                });
            }
            self.events_received.push(format!("{event:?}"));
            Ok(())
        }

        fn on_result(&mut self, _result: &TestRunResult) -> error::Result<()> {
            if self.should_error {
                return Err(error::TestxError::PluginError {
                    message: "mock result error".into(),
                });
            }
            self.result_received = true;
            Ok(())
        }

        fn shutdown(&mut self) -> error::Result<()> {
            self.shutdown_called = true;
            Ok(())
        }
    }

    fn make_result() -> TestRunResult {
        TestRunResult {
            suites: vec![TestSuite {
                name: "test".into(),
                tests: vec![TestCase {
                    name: "test_a".into(),
                    status: TestStatus::Passed,
                    duration: Duration::from_millis(10),
                    error: None,
                }],
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: 0,
        }
    }

    #[test]
    fn manager_new_is_empty() {
        let mgr = PluginManager::new();
        assert_eq!(mgr.plugin_count(), 0);
        assert!(mgr.plugin_names().is_empty());
    }

    #[test]
    fn manager_register() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("test-plugin")));
        assert_eq!(mgr.plugin_count(), 1);
        assert_eq!(mgr.plugin_names(), vec!["test-plugin"]);
    }

    #[test]
    fn manager_dispatch_event() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("p1")));
        mgr.register(Box::new(MockPlugin::new("p2")));

        mgr.dispatch_event(&TestEvent::Warning {
            message: "test warning".into(),
        });

        assert!(mgr.errors().is_empty());
    }

    #[test]
    fn manager_dispatch_result() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("p1")));

        mgr.dispatch_result(&make_result());
        assert!(mgr.errors().is_empty());
    }

    #[test]
    fn manager_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::failing("bad-plugin")));
        mgr.register(Box::new(MockPlugin::new("good-plugin")));

        mgr.dispatch_event(&TestEvent::Warning {
            message: "test".into(),
        });

        assert_eq!(mgr.errors().len(), 1);
        assert_eq!(mgr.errors()[0].plugin_name, "bad-plugin");
    }

    #[test]
    fn manager_shutdown() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("p1")));
        mgr.shutdown_all();
        // No errors from shutdown
        assert!(mgr.errors().is_empty());
    }

    #[test]
    fn manager_remove() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("p1")));
        mgr.register(Box::new(MockPlugin::new("p2")));

        assert!(mgr.remove("p1"));
        assert_eq!(mgr.plugin_count(), 1);
        assert_eq!(mgr.plugin_names(), vec!["p2"]);
    }

    #[test]
    fn manager_remove_nonexistent() {
        let mut mgr = PluginManager::new();
        assert!(!mgr.remove("nope"));
    }

    #[test]
    fn manager_clear_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::failing("bad")));
        mgr.dispatch_event(&TestEvent::Warning {
            message: "x".into(),
        });
        assert_eq!(mgr.errors().len(), 1);
        mgr.clear_errors();
        assert!(mgr.errors().is_empty());
    }

    #[test]
    fn manager_has_fatal_error() {
        let mgr = PluginManager::new();
        assert!(!mgr.has_fatal_error());
    }

    #[test]
    fn plugin_error_display() {
        let err = PluginError::new("test", "something broke".into(), false);
        assert_eq!(format!("{err}"), "[plugin:test] something broke");

        let fatal = PluginError::new("test", "critical".into(), true);
        assert!(format!("{fatal}").contains("(fatal)"));
    }

    #[test]
    fn registry_builtin() {
        let registry = PluginRegistry::builtin();
        assert_eq!(registry.list_available().len(), 4);
        assert!(registry.find("markdown").is_some());
        assert!(registry.find("github").is_some());
        assert!(registry.find("html").is_some());
        assert!(registry.find("notify").is_some());
    }

    #[test]
    fn registry_find_missing() {
        let registry = PluginRegistry::builtin();
        assert!(registry.find("nonexistent").is_none());
    }

    #[test]
    fn registry_custom() {
        let mut registry = PluginRegistry::new();
        registry.register_available(PluginInfo::new("custom", "0.1.0", "A custom plugin"));
        assert_eq!(registry.list_available().len(), 1);
        assert_eq!(registry.find("custom").unwrap().version, "0.1.0");
    }

    #[test]
    fn plugin_info_new() {
        let info = PluginInfo::new("test", "1.0.0", "Test plugin");
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.description, "Test plugin");
    }

    #[test]
    fn manager_multiple_events() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::new("p1")));

        for i in 0..10 {
            mgr.dispatch_event(&TestEvent::Progress {
                message: format!("step {i}"),
                current: i,
                total: 10,
            });
        }

        assert!(mgr.errors().is_empty());
    }

    #[test]
    fn manager_error_on_result() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(MockPlugin::failing("bad")));

        mgr.dispatch_result(&make_result());
        assert_eq!(mgr.errors().len(), 1);
        assert!(mgr.errors()[0].message.contains("on_result"));
    }
}
