# Plugins

testx has a plugin system for custom reporters and adapters.

## Built-in reporters

### Markdown

Generates a Markdown report file with test results.

```toml
# Enable via testx.toml (not yet configurable via CLI)
```

Features: summary table, pass/fail details, timestamps, slowest N tests, error messages. Writes to a file or stdout.

### GitHub Actions

When running in GitHub Actions, testx can:

- Emit `::error` and `::warning` annotations on failures
- Group output with `::group` / `::endgroup`
- Write a summary to `$GITHUB_STEP_SUMMARY`
- Register a problem matcher for inline annotations

### HTML

Generates a self-contained HTML report with:

- Summary cards (passed / failed / skipped / total)
- Visual progress bar
- Expandable test suites
- Dark mode support

### Notify

Sends desktop notifications when tests finish:

- **Linux**: `notify-send`
- **macOS**: `osascript`
- **Windows**: PowerShell

Options: trigger only on failure, set urgency level, configure timeout.

## Custom adapters

Define custom test runners in `testx.toml`:

```toml
[[custom_adapter]]
name = "my-framework"
detect = "myframework.config"
command = "myfw test"
args = ["--verbose"]
parse = "lines"
confidence = 0.5
```

### Output parsers

| Parser  | Description                                                  |
| ------- | ------------------------------------------------------------ |
| `json`  | Expects JSON matching the TestRunResult schema               |
| `junit` | Expects JUnit XML output                                     |
| `tap`   | Expects TAP (Test Anything Protocol)                         |
| `lines` | One test per line with status prefix                         |
| `regex` | Custom regex patterns with capture groups for pass/fail/skip |

## Plugin trait

For Rust-level integration, plugins implement:

```rust
pub trait Plugin: Send {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_event(&mut self, event: &TestEvent) -> Result<()>;
    fn on_result(&mut self, result: &TestRunResult) -> Result<()>;
    fn shutdown(&mut self) -> Result<()> { Ok(()) }
}
```

The plugin manager dispatches test events to all registered plugins during execution.
