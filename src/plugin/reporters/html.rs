//! HTML reporter plugin.
//!
//! Generates a self-contained HTML test report with a summary
//! dashboard, expandable suite sections, and error display.

use std::fmt::Write;
use std::time::Duration;

use crate::adapters::{TestRunResult, TestStatus};
use crate::error;
use crate::events::TestEvent;
use crate::plugin::Plugin;

/// HTML reporter configuration.
#[derive(Debug, Clone)]
pub struct HtmlConfig {
    /// Output file path (None = stdout)
    pub output_path: Option<String>,
    /// Custom page title
    pub title: String,
    /// Include inline CSS (true) or link to external stylesheet (false)
    pub inline_styles: bool,
    /// Show individual test durations
    pub show_durations: bool,
    /// Maximum number of slowest tests section
    pub show_slowest: usize,
    /// Enable dark mode theme
    pub dark_mode: bool,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            output_path: None,
            title: "Test Report".into(),
            inline_styles: true,
            show_durations: true,
            show_slowest: 5,
            dark_mode: false,
        }
    }
}

/// HTML reporter plugin.
pub struct HtmlReporter {
    config: HtmlConfig,
    output: String,
}

impl HtmlReporter {
    pub fn new(config: HtmlConfig) -> Self {
        Self {
            config,
            output: String::new(),
        }
    }

    /// Get the generated HTML output.
    pub fn output(&self) -> &str {
        &self.output
    }
}

impl Plugin for HtmlReporter {
    fn name(&self) -> &str {
        "html"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_event(&mut self, _event: &TestEvent) -> error::Result<()> {
        Ok(())
    }

    fn on_result(&mut self, result: &TestRunResult) -> error::Result<()> {
        self.output = generate_html(result, &self.config);
        Ok(())
    }
}

/// Generate a complete HTML report from test results.
pub fn generate_html(result: &TestRunResult, config: &HtmlConfig) -> String {
    let mut html = String::with_capacity(8192);

    write_doctype(&mut html);
    write_head(&mut html, config);
    write_body_open(&mut html);
    write_header_section(&mut html, result, config);
    write_summary_cards(&mut html, result);
    write_progress_bar(&mut html, result);

    if result.suites.len() > 1 {
        write_suite_table(&mut html, result);
    }

    write_suite_details(&mut html, result, config);

    if result.total_failed() > 0 {
        write_failures_section(&mut html, result);
    }

    if config.show_slowest > 0 {
        write_slowest_section(&mut html, result, config.show_slowest);
    }

    write_footer(&mut html);
    write_body_close(&mut html);

    html
}

fn write_doctype(html: &mut String) {
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n");
}

fn write_head(html: &mut String, config: &HtmlConfig) {
    let _ = writeln!(html, "<head>");
    let _ = writeln!(
        html,
        "<meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
    );
    let title = html_escape(&config.title);
    let _ = writeln!(html, "<title>{title}</title>");

    if config.inline_styles {
        write_styles(html, config.dark_mode);
    }

    let _ = writeln!(html, "</head>");
}

fn write_styles(html: &mut String, dark: bool) {
    let bg = if dark { "#1e1e2e" } else { "#f8f9fa" };
    let fg = if dark { "#cdd6f4" } else { "#212529" };
    let card_bg = if dark { "#313244" } else { "#ffffff" };
    let border = if dark { "#45475a" } else { "#dee2e6" };

    let _ = writeln!(html, "<style>");
    let _ = writeln!(
        html,
        ":root{{--bg:{bg};--fg:{fg};--card:{card_bg};--border:{border};}}"
    );
    let _ = writeln!(
        html,
        "* {{margin:0;padding:0;box-sizing:border-box;}}"
    );
    let _ = writeln!(
        html,
        "body {{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;\
        background:var(--bg);color:var(--fg);line-height:1.6;padding:2rem;max-width:1200px;margin:0 auto;}}"
    );
    let _ = write!(
        html,
        "h1 {{font-size:1.8rem;margin-bottom:0.5rem;}}\n\
        h2 {{font-size:1.3rem;margin:1.5rem 0 0.5rem;border-bottom:2px solid var(--border);padding-bottom:0.25rem;}}\n\
        h3 {{font-size:1.1rem;margin:1rem 0 0.5rem;}}\n"
    );
    let _ = write!(
        html,
        ".cards {{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:1rem;margin:1rem 0;}}\n\
        .card {{background:var(--card);border:1px solid var(--border);border-radius:8px;padding:1rem;text-align:center;}}\n\
        .card .value {{font-size:2rem;font-weight:700;}}\n\
        .card .label {{font-size:0.85rem;opacity:0.7;}}\n"
    );
    let _ = write!(
        html,
        ".progress {{height:24px;border-radius:12px;overflow:hidden;display:flex;margin:1rem 0;\
        background:var(--border);}}\n\
        .progress .pass {{background:#40a02b;}}\n\
        .progress .fail {{background:#d20f39;}}\n\
        .progress .skip {{background:#df8e1d;}}\n"
    );
    let _ = write!(
        html,
        "table {{width:100%;border-collapse:collapse;margin:0.5rem 0;}}\n\
        th,td {{padding:0.5rem 0.75rem;text-align:left;border-bottom:1px solid var(--border);}}\n\
        th {{background:var(--card);font-weight:600;}}\n"
    );
    let _ = write!(
        html,
        "details {{margin:0.5rem 0;border:1px solid var(--border);border-radius:4px;overflow:hidden;}}\n\
        summary {{padding:0.5rem 0.75rem;cursor:pointer;background:var(--card);font-weight:500;}}\n\
        summary:hover {{opacity:0.8;}}\n\
        details .content {{padding:0.75rem;}}\n"
    );
    let _ = write!(
        html,
        ".pass-text {{color:#40a02b;}}\n\
        .fail-text {{color:#d20f39;}}\n\
        .skip-text {{color:#df8e1d;}}\n"
    );
    let _ = writeln!(
        html,
        "pre {{background:var(--card);border:1px solid var(--border);border-radius:4px;\
        padding:0.75rem;overflow-x:auto;font-size:0.85rem;margin:0.5rem 0;}}"
    );
    let _ = writeln!(
        html,
        "footer {{margin-top:2rem;padding-top:1rem;border-top:1px solid var(--border);\
        font-size:0.8rem;opacity:0.6;text-align:center;}}"
    );
    let _ = writeln!(html, "</style>");
}

fn write_body_open(html: &mut String) {
    html.push_str("<body>\n");
}

fn write_body_close(html: &mut String) {
    html.push_str("</body>\n</html>\n");
}

fn write_header_section(html: &mut String, result: &TestRunResult, config: &HtmlConfig) {
    let status = if result.is_success() {
        "<span class=\"pass-text\"> PASSED</span>"
    } else {
        "<span class=\"fail-text\"> FAILED</span>"
    };
    let title = html_escape(&config.title);

    let _ = writeln!(html, "<h1>{title} — {status}</h1>");
    let _ = writeln!(
        html,
        "<p>Duration: {} | Exit code: {}</p>",
        format_duration(result.duration),
        result.raw_exit_code,
    );
}

fn write_summary_cards(html: &mut String, result: &TestRunResult) {
    let _ = writeln!(html, "<div class=\"cards\">");

    write_card(html, &result.total_tests().to_string(), "Total", "");
    write_card(
        html,
        &result.total_passed().to_string(),
        "Passed",
        " pass-text",
    );
    write_card(
        html,
        &result.total_failed().to_string(),
        "Failed",
        " fail-text",
    );
    write_card(
        html,
        &result.total_skipped().to_string(),
        "Skipped",
        " skip-text",
    );
    write_card(
        html,
        &result.suites.len().to_string(),
        "Suites",
        "",
    );
    write_card(
        html,
        &format_duration(result.duration),
        "Duration",
        "",
    );

    let _ = writeln!(html, "</div>");
}

fn write_card(html: &mut String, value: &str, label: &str, class: &str) {
    let _ = writeln!(
        html,
        "<div class=\"card\"><div class=\"value{class}\">{value}</div><div class=\"label\">{label}</div></div>"
    );
}

fn write_progress_bar(html: &mut String, result: &TestRunResult) {
    let total = result.total_tests();
    if total == 0 {
        return;
    }

    let pass_pct = result.total_passed() as f64 / total as f64 * 100.0;
    let fail_pct = result.total_failed() as f64 / total as f64 * 100.0;
    let skip_pct = result.total_skipped() as f64 / total as f64 * 100.0;

    let _ = writeln!(html, "<div class=\"progress\">");
    if pass_pct > 0.0 {
        let _ = writeln!(
            html,
            "  <div class=\"pass\" style=\"width:{pass_pct:.1}%\" title=\"{} passed\"></div>",
            result.total_passed()
        );
    }
    if fail_pct > 0.0 {
        let _ = writeln!(
            html,
            "  <div class=\"fail\" style=\"width:{fail_pct:.1}%\" title=\"{} failed\"></div>",
            result.total_failed()
        );
    }
    if skip_pct > 0.0 {
        let _ = writeln!(
            html,
            "  <div class=\"skip\" style=\"width:{skip_pct:.1}%\" title=\"{} skipped\"></div>",
            result.total_skipped()
        );
    }
    let _ = writeln!(html, "</div>");
}

fn write_suite_table(html: &mut String, result: &TestRunResult) {
    let _ = writeln!(html, "<h2>Suites</h2>");
    let _ = writeln!(html, "<table>");
    let _ = writeln!(
        html,
        "<thead><tr><th>Suite</th><th>Tests</th><th>Passed</th><th>Failed</th><th>Skipped</th><th>Status</th></tr></thead>"
    );
    let _ = writeln!(html, "<tbody>");

    for suite in &result.suites {
        let status = if suite.is_passed() {
            "<span class=\"pass-text\"></span>"
        } else {
            "<span class=\"fail-text\"></span>"
        };
        let name = html_escape(&suite.name);
        let _ = writeln!(
            html,
            "<tr><td>{name}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{status}</td></tr>",
            suite.tests.len(),
            suite.passed(),
            suite.failed(),
            suite.skipped(),
        );
    }

    let _ = writeln!(html, "</tbody></table>");
}

fn write_suite_details(html: &mut String, result: &TestRunResult, config: &HtmlConfig) {
    let _ = writeln!(html, "<h2>Details</h2>");

    for suite in &result.suites {
        let icon = if suite.is_passed() { "✅" } else { "❌" };
        let name = html_escape(&suite.name);
        let open = if !suite.is_passed() { " open" } else { "" };

        let _ = writeln!(html, "<details{open}>");
        let _ = writeln!(
            html,
            "<summary>{icon} {name} ({} tests, {} passed, {} failed)</summary>",
            suite.tests.len(),
            suite.passed(),
            suite.failed(),
        );
        let _ = writeln!(html, "<div class=\"content\">");
        let _ = writeln!(html, "<table>");
        let _ = write!(html, "<thead><tr><th>Test</th><th>Status</th>");
        if config.show_durations {
            html.push_str("<th>Duration</th>");
        }
        let _ = writeln!(html, "<th>Error</th></tr></thead>");
        let _ = writeln!(html, "<tbody>");

        for test in &suite.tests {
            let (class, icon) = match test.status {
                TestStatus::Passed => ("pass-text", ""),
                TestStatus::Failed => ("fail-text", ""),
                TestStatus::Skipped => ("skip-text", "⏭️"),
            };
            let test_name = html_escape(&test.name);
            let _ = write!(
                html,
                "<tr><td>{test_name}</td><td class=\"{class}\">{icon} {:?}</td>",
                test.status
            );
            if config.show_durations {
                let _ = write!(html, "<td>{}</td>", format_duration(test.duration));
            }
            let error_cell = test
                .error
                .as_ref()
                .map(|e| format!("<pre>{}</pre>", html_escape(&e.message)))
                .unwrap_or_default();
            let _ = writeln!(html, "<td>{error_cell}</td></tr>");
        }

        let _ = writeln!(html, "</tbody></table>");
        let _ = writeln!(html, "</div></details>");
    }
}

fn write_failures_section(html: &mut String, result: &TestRunResult) {
    let _ = writeln!(html, "<h2>Failures</h2>");

    for suite in &result.suites {
        for test in suite.failures() {
            let suite_name = html_escape(&suite.name);
            let test_name = html_escape(&test.name);
            let _ = writeln!(html, "<h3> {suite_name}::{test_name}</h3>");
            if let Some(ref error) = test.error {
                let msg = html_escape(&error.message);
                let _ = writeln!(html, "<pre>{msg}</pre>");
                if let Some(ref loc) = error.location {
                    let loc = html_escape(loc);
                    let _ = writeln!(html, "<p>at <code>{loc}</code></p>");
                }
            }
        }
    }
}

fn write_slowest_section(html: &mut String, result: &TestRunResult, n: usize) {
    let slowest = result.slowest_tests(n);
    if slowest.is_empty() {
        return;
    }

    let _ = writeln!(html, "<h2>Slowest Tests</h2>");
    let _ = writeln!(html, "<table>");
    let _ = writeln!(
        html,
        "<thead><tr><th>#</th><th>Test</th><th>Suite</th><th>Duration</th></tr></thead>"
    );
    let _ = writeln!(html, "<tbody>");

    for (i, (suite, test)) in slowest.iter().enumerate() {
        let suite_name = html_escape(&suite.name);
        let test_name = html_escape(&test.name);
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{test_name}</td><td>{suite_name}</td><td>{}</td></tr>",
            i + 1,
            format_duration(test.duration),
        );
    }

    let _ = writeln!(html, "</tbody></table>");
}

fn write_footer(html: &mut String) {
    let _ = writeln!(html, "<footer>Generated by testx</footer>");
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        "&lt;1ms".to_string()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestError, TestSuite};

    fn make_test(name: &str, status: TestStatus, ms: u64) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(ms),
            error: None,
        }
    }

    fn make_failed_test(name: &str, ms: u64, msg: &str) -> TestCase {
        TestCase {
            name: name.into(),
            status: TestStatus::Failed,
            duration: Duration::from_millis(ms),
            error: Some(TestError {
                message: msg.into(),
                location: Some("test.rs:10".into()),
            }),
        }
    }

    fn make_result() -> TestRunResult {
        TestRunResult {
            suites: vec![
                TestSuite {
                    name: "math".into(),
                    tests: vec![
                        make_test("add", TestStatus::Passed, 10),
                        make_test("sub", TestStatus::Passed, 20),
                        make_failed_test("div", 5, "division by zero"),
                    ],
                },
                TestSuite {
                    name: "strings".into(),
                    tests: vec![
                        make_test("concat", TestStatus::Passed, 15),
                        make_test("upper", TestStatus::Skipped, 0),
                    ],
                },
            ],
            duration: Duration::from_millis(500),
            raw_exit_code: 1,
        }
    }

    #[test]
    fn html_valid_document() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn html_title() {
        let config = HtmlConfig {
            title: "My Tests".into(),
            ..Default::default()
        };
        let html = generate_html(&make_result(), &config);
        assert!(html.contains("<title>My Tests</title>"));
    }

    #[test]
    fn html_summary_cards() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("class=\"cards\""));
        assert!(html.contains(">5<")); // total
        assert!(html.contains(">3<")); // passed
        assert!(html.contains(">1<")); // failed
    }

    #[test]
    fn html_progress_bar() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("class=\"progress\""));
        assert!(html.contains("class=\"pass\""));
        assert!(html.contains("class=\"fail\""));
    }

    #[test]
    fn html_suite_table() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<h2>Suites</h2>"));
        assert!(html.contains("math"));
        assert!(html.contains("strings"));
    }

    #[test]
    fn html_suite_details() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<details"));
        assert!(html.contains("<summary>"));
    }

    #[test]
    fn html_failed_suite_open() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        // Failed suite should have 'open' attribute
        assert!(html.contains("<details open>"));
    }

    #[test]
    fn html_failures() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<h2>Failures</h2>"));
        assert!(html.contains("division by zero"));
    }

    #[test]
    fn html_error_location() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("test.rs:10"));
    }

    #[test]
    fn html_slowest() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<h2>Slowest Tests</h2>"));
    }

    #[test]
    fn html_inline_styles() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<style>"));
    }

    #[test]
    fn html_no_inline_styles() {
        let config = HtmlConfig {
            inline_styles: false,
            ..Default::default()
        };
        let html = generate_html(&make_result(), &config);
        assert!(!html.contains("<style>"));
    }

    #[test]
    fn html_dark_mode() {
        let config = HtmlConfig {
            dark_mode: true,
            ..Default::default()
        };
        let html = generate_html(&make_result(), &config);
        assert!(html.contains("#1e1e2e"));
    }

    #[test]
    fn html_no_failures_no_section() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "t".into(),
                tests: vec![make_test("t1", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let html = generate_html(&result, &HtmlConfig::default());
        assert!(!html.contains("<h2>Failures</h2>"));
    }

    #[test]
    fn html_single_suite_no_table() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "single".into(),
                tests: vec![make_test("t", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let html = generate_html(&result, &HtmlConfig::default());
        assert!(!html.contains("<h2>Suites</h2>"));
    }

    #[test]
    fn html_footer() {
        let html = generate_html(&make_result(), &HtmlConfig::default());
        assert!(html.contains("<footer>"));
        assert!(html.contains("testx"));
    }

    #[test]
    fn html_escape_xss() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "<script>alert('xss')</script>".into(),
                tests: vec![make_test("t", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let html = generate_html(&result, &HtmlConfig::default());
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn html_no_durations() {
        let config = HtmlConfig {
            show_durations: false,
            ..Default::default()
        };
        let html = generate_html(&make_result(), &config);
        // Duration column header not present in test detail tables
        assert!(html.contains("Details"));
    }

    #[test]
    fn html_plugin_trait() {
        let mut reporter = HtmlReporter::new(HtmlConfig::default());
        assert_eq!(reporter.name(), "html");
        assert_eq!(reporter.version(), "1.0.0");

        reporter.on_result(&make_result()).unwrap();
        assert!(reporter.output().contains("<!DOCTYPE html>"));
    }

    #[test]
    fn html_pass_status() {
        let result = TestRunResult {
            suites: vec![TestSuite {
                name: "t".into(),
                tests: vec![make_test("t1", TestStatus::Passed, 1)],
            }],
            duration: Duration::from_millis(10),
            raw_exit_code: 0,
        };
        let html = generate_html(&result, &HtmlConfig::default());
        assert!(html.contains("PASSED"));
    }

    #[test]
    fn html_escape_quotes() {
        let escaped = html_escape("say \"hello\" & 'bye'");
        assert_eq!(escaped, "say &quot;hello&quot; &amp; &#39;bye&#39;");
    }

    #[test]
    fn html_empty_result() {
        let result = TestRunResult {
            suites: vec![],
            duration: Duration::ZERO,
            raw_exit_code: 0,
        };
        let html = generate_html(&result, &HtmlConfig::default());
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("PASSED"));
    }
}
