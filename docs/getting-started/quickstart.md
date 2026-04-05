# Quick Start

This guide walks you through using testx for the first time. By the end, you'll know how to run tests, understand the output, and use the most common features.

---

## Your first test run

Open a terminal, navigate to any project that has tests, and run:

```bash
testx
```

That's it. testx automatically:

1. **Detects** your programming language (Python, Rust, JavaScript, Go, etc.)
2. **Identifies** the test framework (pytest, Jest, cargo test, go test, etc.)
3. **Runs** your tests
4. **Formats** the output in a clear, readable way

You'll see output like this:

```
 ● Python (pytest) — 12 tests

 ✓ test_login_success                          0.03s
 ✓ test_login_invalid_password                 0.02s
 ✓ test_signup_new_user                        0.05s
 ✗ test_password_reset                         0.04s
   AssertionError: expected 200, got 404

 11 passed · 1 failed · 0 skipped               0.8s
```

---

## Check what testx detects (without running anything)

If you want to see what testx would do before actually running tests:

```bash
testx detect
```

Output:

```
Detected: Python (pytest) — confidence 94%
```

This is useful when you're in an unfamiliar project or want to confirm testx is picking up the right framework.

---

## Pass arguments to the underlying test runner

Sometimes you need to pass flags directly to the test runner (pytest, Jest, cargo test, etc.). Use `--` to separate testx flags from test-runner flags:

```bash
# Pass "-k test_login" to pytest to filter tests
testx -- -k "test_login"

# Pass "--test-threads=1" to cargo test
testx -- --test-threads=1

# Run a specific test file with Jest
testx -- src/auth.test.js

# Pass verbose flag to go test
testx -- -v
```

Everything after `--` goes directly to whichever test runner testx detected.

---

## Run tests in a different directory

By default, testx looks at the current directory. To test a project somewhere else:

```bash
testx -p /path/to/your/project
```

This is handy when you have multiple projects and don't want to `cd` back and forth.

---

## Generate a config file

If you want to customize testx's behavior for a project, generate a config file:

```bash
testx init
```

This creates a `testx.toml` in the current directory with sensible defaults. You can then edit it to override the detected framework, set timeouts, configure output formats, and more. See the [Configuration](../guide/configuration.md) guide for full details.

---

## Common workflows

Here are the most frequently used features:

### Find slow tests

```bash
# Show the 5 slowest tests
testx --slowest 5
```

This helps you find which tests are slowing down your suite.

### Set a timeout

```bash
# Kill the test run if it takes more than 60 seconds
testx --timeout 60
```

Useful in CI or when a test might hang.

### Watch mode

```bash
# Re-run tests automatically when files change
testx -w
```

Great for local development — save a file and tests re-run instantly.

### Retry flaky tests

```bash
# Retry failed tests up to 3 times
testx --retries 3
```

If a test fails, testx will re-run it up to 3 more times. A test passes if any retry succeeds.

### See the raw output

```bash
# Show the original test runner output (no formatting)
testx --raw
```

Useful for debugging or when you need the exact output from pytest/jest/etc.

### Verbose mode

```bash
# Show the detected command and extra details
testx -v
```

This shows you exactly what command testx is running under the hood.

---

## Monorepo / workspace support

If you have a monorepo with multiple projects (e.g., a frontend + backend + shared library), testx can discover and test them all:

```bash
# Discover and run tests in all sub-projects
testx workspace
```

```bash
# Just list what testx found (without running tests)
testx workspace --list
```

```bash
# Only test specific languages
testx workspace --filter rust,python
```

See the [Workspace](../guide/workspace.md) guide for more details on monorepo workflows.

---

## Test history and analytics

testx tracks your test results over time, so you can spot trends:

```bash
# See a summary of recent test runs
testx history

# Find tests that pass sometimes and fail other times (flaky tests)
testx history flaky

# Get a health score for your test suite
testx history health
```

See the [History](../guide/history.md) guide for more.

---

## Output formats for CI

By default, testx shows pretty-printed output for your terminal. You can also output structured formats for CI systems:

```bash
# JSON output (for custom tooling)
testx --format json

# JUnit XML (for CI dashboards like GitHub Actions, Jenkins)
testx --format junit

# TAP (Test Anything Protocol)
testx --format tap
```

See the [Output Formats](../guide/output-formats.md) guide for details.

---

## Next steps

Now that you've got the basics, explore these guides:

| Guide                                              | What you'll learn                                                     |
| -------------------------------------------------- | --------------------------------------------------------------------- |
| [Configuration](../guide/configuration.md)         | Customize testx with `testx.toml` — override frameworks, set defaults |
| [Output Formats](../guide/output-formats.md)       | JSON, JUnit XML, and TAP output for CI                                |
| [CI Sharding](../guide/sharding.md)                | Split tests across CI nodes for faster pipelines                      |
| [Flaky Test Detection](../guide/stress-testing.md) | Run tests repeatedly to find intermittent failures                    |
| [Cache & Smart Runs](../guide/caching.md)          | Skip tests that haven't changed                                       |
| [Impact Analysis](../guide/impact-analysis.md)     | Only run tests affected by your code changes                          |
| [Monorepo Support](../guide/workspace.md)          | Test across multiple projects                                         |
| [Test History](../guide/history.md)                | Analytics, trends, and health scoring                                 |
| [Interactive Picker](../guide/picker.md)           | Fuzzy-search and pick tests interactively                             |
| [Plugins](../guide/plugins.md)                     | Custom reporters and adapters                                         |
