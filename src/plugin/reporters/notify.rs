//! Desktop notification reporter plugin.
//!
//! Sends OS-level notifications on test completion using
//! `notify-send` (Linux), `osascript` (macOS), or
//! `powershell` (Windows).

#[cfg(not(test))]
use std::process::Command;

use crate::adapters::TestRunResult;
use crate::error;
use crate::events::TestEvent;
use crate::plugin::Plugin;

/// Notification reporter configuration.
#[derive(Debug, Clone)]
pub struct NotifyConfig {
    /// Only notify on failure
    pub on_failure_only: bool,
    /// Custom notification title prefix
    pub title_prefix: String,
    /// Notification urgency on failure: "low", "normal", "critical"
    pub urgency: String,
    /// Timeout in milliseconds (0 = system default)
    pub timeout_ms: u32,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            on_failure_only: false,
            title_prefix: "testx".into(),
            urgency: "normal".into(),
            timeout_ms: 5000,
        }
    }
}

/// Desktop notification reporter plugin.
pub struct NotifyReporter {
    config: NotifyConfig,
    last_notification: Option<Notification>,
}

/// Captured notification for testing / inspection.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: String,
}

impl NotifyReporter {
    pub fn new(config: NotifyConfig) -> Self {
        Self {
            config,
            last_notification: None,
        }
    }

    /// Get the last notification that was built (for testing).
    pub fn last_notification(&self) -> Option<&Notification> {
        self.last_notification.as_ref()
    }
}

impl Plugin for NotifyReporter {
    fn name(&self) -> &str {
        "notify"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_event(&mut self, _event: &TestEvent) -> error::Result<()> {
        Ok(())
    }

    fn on_result(&mut self, result: &TestRunResult) -> error::Result<()> {
        if self.config.on_failure_only && result.is_success() {
            return Ok(());
        }

        let notification = build_notification(result, &self.config);
        self.last_notification = Some(notification.clone());

        // Best-effort send — don't fire real notifications during tests
        #[cfg(not(test))]
        {
            let _ = send_notification(&notification, &self.config);
        }
        Ok(())
    }
}

/// Build a notification from test results.
pub fn build_notification(result: &TestRunResult, config: &NotifyConfig) -> Notification {
    let status = if result.is_success() {
        "PASSED"
    } else {
        "FAILED"
    };

    let title = format!("{} — {status}", config.title_prefix);

    let body = format!(
        "{} tests: {} passed, {} failed, {} skipped\nDuration: {:.2}s",
        result.total_tests(),
        result.total_passed(),
        result.total_failed(),
        result.total_skipped(),
        result.duration.as_secs_f64(),
    );

    let urgency = if result.is_success() {
        "low".to_string()
    } else {
        config.urgency.clone()
    };

    Notification {
        title,
        body,
        urgency,
    }
}

/// Send a notification using OS-specific tools.
#[cfg(not(test))]
fn send_notification(notification: &Notification, config: &NotifyConfig) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        send_linux(notification, config)?;
    }

    #[cfg(target_os = "macos")]
    {
        send_macos(notification, config)?;
    }

    #[cfg(target_os = "windows")]
    {
        send_windows(notification, config)?;
    }

    Ok(())
}

/// Send notification via `notify-send` on Linux.
#[cfg(all(target_os = "linux", not(test)))]
fn send_linux(notification: &Notification, config: &NotifyConfig) -> std::io::Result<()> {
    let mut cmd = Command::new("notify-send");
    cmd.arg("--urgency").arg(&notification.urgency);

    if config.timeout_ms > 0 {
        cmd.arg("--expire-time").arg(config.timeout_ms.to_string());
    }

    cmd.arg(&notification.title).arg(&notification.body);

    cmd.output()?;
    Ok(())
}

/// Send notification via `osascript` on macOS.
#[cfg(all(target_os = "macos", not(test)))]
fn send_macos(notification: &Notification, _config: &NotifyConfig) -> std::io::Result<()> {
    // Use quoted form to prevent AppleScript injection.
    // Escape backslashes first, then double-quotes.
    fn applescript_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(&notification.body),
        applescript_escape(&notification.title),
    );

    Command::new("osascript").arg("-e").arg(&script).output()?;
    Ok(())
}

/// Send notification via PowerShell on Windows.
#[cfg(all(target_os = "windows", not(test)))]
fn send_windows(notification: &Notification, _config: &NotifyConfig) -> std::io::Result<()> {
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    let title = xml_escape(&notification.title);
    let body = xml_escape(&notification.body);

    // Write XML to a temp file to avoid PowerShell injection via here-string
    // terminators ("'@") in test names or output.
    let tmp = std::env::temp_dir().join("testx_toast.xml");
    let xml_content = format!(
        "<toast><visual><binding template=\"ToastText02\"><text id=\"1\">{}</text><text id=\"2\">{}</text></binding></visual></toast>",
        title, body,
    );
    std::fs::write(&tmp, &xml_content)?;

    let script = format!(
        "[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime] | Out-Null; \
        $xml = [xml](Get-Content -Raw -LiteralPath '{}'); \
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('testx').Show($toast)",
        tmp.display(),
    );

    Command::new("powershell")
        .arg("-Command")
        .arg(&script)
        .output()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TestStatus;
    use crate::adapters::{TestCase, TestSuite};
    use std::time::Duration;

    fn make_test(name: &str, status: TestStatus, ms: u64) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(ms),
            error: None,
        }
    }

    fn passing_result() -> TestRunResult {
        TestRunResult {
            suites: vec![TestSuite {
                name: "math".into(),
                tests: vec![
                    make_test("add", TestStatus::Passed, 10),
                    make_test("sub", TestStatus::Passed, 20),
                ],
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: 0,
        }
    }

    fn failing_result() -> TestRunResult {
        TestRunResult {
            suites: vec![TestSuite {
                name: "math".into(),
                tests: vec![
                    make_test("add", TestStatus::Passed, 10),
                    make_test("div", TestStatus::Failed, 5),
                ],
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: 1,
        }
    }

    #[test]
    fn notification_pass_title() {
        let n = build_notification(&passing_result(), &NotifyConfig::default());
        assert!(n.title.contains("PASSED"));
        assert!(n.title.contains("testx"));
    }

    #[test]
    fn notification_fail_title() {
        let n = build_notification(&failing_result(), &NotifyConfig::default());
        assert!(n.title.contains("FAILED"));
    }

    #[test]
    fn notification_body_counts() {
        let n = build_notification(&failing_result(), &NotifyConfig::default());
        assert!(n.body.contains("2 tests"));
        assert!(n.body.contains("1 passed"));
        assert!(n.body.contains("1 failed"));
    }

    #[test]
    fn notification_urgency_pass() {
        let n = build_notification(&passing_result(), &NotifyConfig::default());
        assert_eq!(n.urgency, "low");
    }

    #[test]
    fn notification_urgency_fail() {
        let n = build_notification(&failing_result(), &NotifyConfig::default());
        assert_eq!(n.urgency, "normal");
    }

    #[test]
    fn notification_custom_urgency() {
        let config = NotifyConfig {
            urgency: "critical".into(),
            ..Default::default()
        };
        let n = build_notification(&failing_result(), &config);
        assert_eq!(n.urgency, "critical");
    }

    #[test]
    fn notification_custom_prefix() {
        let config = NotifyConfig {
            title_prefix: "mytest".into(),
            ..Default::default()
        };
        let n = build_notification(&passing_result(), &config);
        assert!(n.title.starts_with("mytest"));
    }

    #[test]
    fn plugin_on_failure_only_skip_pass() {
        let mut reporter = NotifyReporter::new(NotifyConfig {
            on_failure_only: true,
            ..Default::default()
        });
        reporter.on_result(&passing_result()).unwrap();
        assert!(reporter.last_notification().is_none());
    }

    #[test]
    fn plugin_on_failure_only_send_fail() {
        let mut reporter = NotifyReporter::new(NotifyConfig {
            on_failure_only: true,
            ..Default::default()
        });
        reporter.on_result(&failing_result()).unwrap();
        assert!(reporter.last_notification().is_some());
    }

    #[test]
    fn plugin_always_notify() {
        let mut reporter = NotifyReporter::new(NotifyConfig::default());
        reporter.on_result(&passing_result()).unwrap();
        assert!(reporter.last_notification().is_some());
    }

    #[test]
    fn plugin_name_version() {
        let reporter = NotifyReporter::new(NotifyConfig::default());
        assert_eq!(reporter.name(), "notify");
        assert_eq!(reporter.version(), "1.0.0");
    }

    #[test]
    fn notification_body_duration() {
        let n = build_notification(&passing_result(), &NotifyConfig::default());
        assert!(n.body.contains("Duration:"));
    }

    #[test]
    fn notification_skipped_count() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "t".into(),
                tests: vec![
                    make_test("t1", TestStatus::Passed, 1),
                    make_test("t2", TestStatus::Skipped, 0),
                ],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.body.contains("1 skipped"));
    }

    // ─── Edge Case Tests ────────────────────────────────────────────────

    #[test]
    fn notification_empty_result() {
        let result = TestRunResult {
            suites: vec![],
            duration: Duration::ZERO,
            raw_exit_code: 0,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.title.contains("PASSED"));
        assert!(n.body.contains("0 tests"));
        assert!(n.body.contains("0 passed"));
        assert!(n.body.contains("0 failed"));
        assert!(n.body.contains("0 skipped"));
    }

    #[test]
    fn notification_all_skipped() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "s".into(),
                tests: vec![
                    make_test("a", TestStatus::Skipped, 0),
                    make_test("b", TestStatus::Skipped, 0),
                ],
            }],
            duration: Duration::from_millis(1),
            raw_exit_code: 0,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.title.contains("PASSED"));
        assert!(n.body.contains("2 skipped"));
        assert_eq!(n.urgency, "low");
    }

    #[test]
    fn notification_empty_prefix() {
        let config = NotifyConfig {
            title_prefix: "".into(),
            ..Default::default()
        };
        let n = build_notification(&passing_result(), &config);
        assert!(n.title.contains("PASSED"));
        assert!(n.title.starts_with(" — PASSED"));
    }

    #[test]
    fn notification_custom_urgency_ignored_on_pass() {
        let config = NotifyConfig {
            urgency: "critical".into(),
            ..Default::default()
        };
        let n = build_notification(&passing_result(), &config);
        // Pass always gets "low" regardless of config
        assert_eq!(n.urgency, "low");
    }

    #[test]
    fn notification_zero_duration() {
        let result = TestRunResult {
            suites: vec![],
            duration: Duration::ZERO,
            raw_exit_code: 0,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.body.contains("0.00s"));
    }

    #[test]
    fn notification_long_duration() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "s".into(),
                tests: vec![make_test("t", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_secs(600),
            raw_exit_code: 0,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.body.contains("600.00s"));
    }

    #[test]
    fn plugin_on_event_noop() {
        let mut r = NotifyReporter::new(NotifyConfig::default());
        assert!(
            r.on_event(&crate::events::TestEvent::Warning {
                message: "x".into()
            })
            .is_ok()
        );
        assert!(r.last_notification().is_none());
    }

    #[test]
    fn plugin_shutdown_ok() {
        let r = NotifyReporter::new(NotifyConfig::default());
        // shutdown is a default trait method — ensure it returns Ok
        let mut r = r;
        assert!(Plugin::shutdown(&mut r).is_ok());
    }

    #[test]
    fn plugin_multiple_on_result_keeps_last() {
        let mut r = NotifyReporter::new(NotifyConfig::default());
        r.on_result(&passing_result()).unwrap();
        assert!(r.last_notification().unwrap().title.contains("PASSED"));

        r.on_result(&failing_result()).unwrap();
        assert!(r.last_notification().unwrap().title.contains("FAILED"));
    }

    #[test]
    fn plugin_on_failure_only_false_sends_on_pass() {
        let mut r = NotifyReporter::new(NotifyConfig {
            on_failure_only: false,
            ..Default::default()
        });
        r.on_result(&passing_result()).unwrap();
        assert!(r.last_notification().is_some());
    }

    #[test]
    fn notification_config_default_values() {
        let c = NotifyConfig::default();
        assert!(!c.on_failure_only);
        assert_eq!(c.title_prefix, "testx");
        assert_eq!(c.urgency, "normal");
        assert_eq!(c.timeout_ms, 5000);
    }

    #[test]
    fn notification_config_clone() {
        let c = NotifyConfig {
            on_failure_only: true,
            title_prefix: "custom".into(),
            urgency: "critical".into(),
            timeout_ms: 0,
        };
        let c2 = c.clone();
        assert_eq!(c2.title_prefix, "custom");
        assert_eq!(c2.timeout_ms, 0);
        assert!(c2.on_failure_only);
    }

    #[test]
    fn notification_struct_equality() {
        let n1 = Notification {
            title: "t".into(),
            body: "b".into(),
            urgency: "low".into(),
        };
        let n2 = n1.clone();
        assert_eq!(n1, n2);
    }

    #[test]
    fn notification_struct_debug() {
        let n = Notification {
            title: "t".into(),
            body: "b".into(),
            urgency: "low".into(),
        };
        let dbg = format!("{n:?}");
        assert!(dbg.contains("Notification"));
    }

    #[test]
    fn notification_many_suites() {
        let suites: Vec<TestSuite> = (0..5)
            .map(|i| TestSuite {
                name: format!("suite_{i}"),
                tests: vec![
                    make_test(&format!("p_{i}"), TestStatus::Passed, 1),
                    make_test(&format!("f_{i}"), TestStatus::Failed, 1),
                ],
            })
            .collect();
        let result = TestRunResult {
            suites,
            duration: Duration::from_millis(50),
            raw_exit_code: 1,
        };
        let n = build_notification(&result, &NotifyConfig::default());
        assert!(n.body.contains("10 tests"));
        assert!(n.body.contains("5 passed"));
        assert!(n.body.contains("5 failed"));
        assert!(n.title.contains("FAILED"));
    }
}
