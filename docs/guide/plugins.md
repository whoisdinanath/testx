# Plugins

testx has a plugin system for **reporters** (how test results are displayed) and **custom adapters** (how test frameworks are detected and run).

---

## Built-in reporters

Reporters transform test results into different formats or outputs. You activate them with `--reporter`:

```bash
testx --reporter github
testx --reporter html
testx --reporter markdown
testx --reporter notify
```

### GitHub Actions reporter

When running in GitHub Actions, this reporter integrates with Actions features:

- **Annotations:** Emits `::error` and `::warning` annotations so failures appear inline on your PR
- **Output grouping:** Uses `::group` / `::endgroup` for clean collapsible output
- **Job summary:** Writes a Markdown summary to `$GITHUB_STEP_SUMMARY`
- **Problem matchers:** Registers matchers for inline annotations in the "Files changed" tab

**Example workflow:**

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: testx --reporter github
```

### Markdown reporter

Generates a Markdown report file with test results. Includes:

- Summary table (passed / failed / skipped / total)
- Individual test details with pass/fail status
- Timestamps and durations
- Slowest N tests
- Error messages for failures

Useful for generating reports that get committed to a repository or posted as PR comments.

### HTML reporter

Generates a self-contained HTML file (no external dependencies) with:

- Summary cards showing passed / failed / skipped / total counts
- A visual progress bar
- Expandable test suite sections
- Dark mode support

Great for sharing test results with non-technical stakeholders or archiving reports.

### Notify reporter

Sends a **desktop notification** when tests finish, so you don't have to keep watching the terminal.

- **Linux:** Uses `notify-send`
- **macOS:** Uses `osascript`
- **Windows:** Uses PowerShell

You can configure it to only notify on failures, set urgency levels, and customize the timeout.

---

## Custom adapters

If your project uses a test framework that testx doesn't natively support, you can define a custom adapter in `testx.toml`. This tells testx how to detect and run your framework.

### Basic adapter

```toml
[[custom_adapter]]
name = "my-framework"
detect = "myframework.config"    # If this file exists, use this adapter
command = "myfw test"            # Command to run tests
args = ["--verbose"]             # Extra arguments
output = "lines"                 # How to parse output (see below)
confidence = 0.5                 # Detection confidence (0.0 – 1.0)
check = "myfw --version"         # Verify the runner is installed
```

### Advanced detection

For more precise detection, use multiple signals (all conditions must be met):

```toml
[[custom_adapter]]
name = "make-test"
command = "make test"
output = "lines"
confidence = 0.85

[custom_adapter.detect]
files = ["Makefile"]                  # At least one file must exist
commands = ["make --version"]         # All commands must succeed (exit 0)

[[custom_adapter.detect.content]]
file = "Makefile"
contains = "test:"                    # File must contain this string
```

### Global adapters

To make an adapter available in **all** your projects, place `.toml` files in:

```
~/.config/testx/adapters/
```

(Or `$XDG_CONFIG_HOME/testx/adapters/` on Linux.)

### Managing adapters

```bash
# List all registered adapters (built-in + project + global)
testx adapters

# Disable custom adapter loading (use only built-in adapters)
testx --no-custom-adapters
```

See the [Configuration](configuration.md) guide for the full custom adapter reference.

---

## Output parsers

When defining a custom adapter, the `output` field tells testx how to parse the test runner's output:

| Parser  | Description                                                                   |
| ------- | ----------------------------------------------------------------------------- |
| `json`  | Expects JSON matching testx's `TestRunResult` schema                          |
| `junit` | Expects JUnit XML output                                                      |
| `tap`   | Expects TAP (Test Anything Protocol) output                                   |
| `lines` | Treats each line as a test result with a status prefix — simplest option      |
| `regex` | Custom regex patterns with named capture groups for pass/fail/skip extraction |

For most custom adapters, start with `output = "lines"`. If your framework can output JUnit, TAP, or JSON, use those for richer structured results.

---

## Plugin trait (Rust API)

If you're extending testx at the Rust level (contributing to testx or building plugins), plugins implement this trait:

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
