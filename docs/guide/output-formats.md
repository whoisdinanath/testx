# Output Formats

testx can output test results in four formats. The default (`pretty`) is designed for human reading in a terminal. The other three (`json`, `junit`, `tap`) are structured formats for CI systems, scripts, and external tools.

---

## Pretty (default)

```bash
testx
```

This is the default format — clean, colored terminal output with pass/fail icons, test names, and timing information:

```
 ● Python (pytest) — 3 tests

 ✓ test_add                                    0.001s
 ✓ test_subtract                               0.001s
 ✗ test_divide_by_zero                         0.002s
   ZeroDivisionError: division by zero

 2 passed · 1 failed · 0 skipped                0.12s
```

**When to use:** Local development, manual test runs, anywhere you're reading the output yourself.

**Tip:** Combine with `--slowest N` to see which tests take the longest:

```bash
testx --slowest 5
```

---

## JSON

```bash
testx -o json
```

Machine-readable structured output. Every test result, timing, and status is captured in a JSON object:

```json
{
  "suites": [
    {
      "name": "tests/test_math.py",
      "tests": [
        { "name": "test_add", "status": "passed", "duration": 0.001 },
        { "name": "test_subtract", "status": "passed", "duration": 0.001 },
        { "name": "test_divide_by_zero", "status": "failed", "duration": 0.002 }
      ]
    }
  ],
  "duration": 0.12,
  "exit_code": 1
}
```

**When to use:** Custom dashboards, scripts that process test results, piping into `jq` or other tools.

**Example — extract just the failed tests with `jq`:**

```bash
testx -o json | jq '.suites[].tests[] | select(.status == "failed")'
```

**Example — save results to a file:**

```bash
testx -o json > test-results.json
```

---

## JUnit XML

```bash
testx -o junit > test-results.xml
```

Standard JUnit XML format, the de facto standard for CI test reporting. Virtually every CI system can parse this format and display results in its dashboard.

**When to use:** CI pipelines — GitHub Actions, Jenkins, GitLab CI, CircleCI, Azure Pipelines, etc.

**Example — GitHub Actions integration:**

```yaml
# .github/workflows/test.yml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run tests
        run: testx -o junit > test-results.xml
      - name: Upload test results
        uses: actions/upload-artifact@v4
        with:
          name: test-results
          path: test-results.xml
        if: always() # Upload even if tests fail
```

---

## TAP (Test Anything Protocol)

```bash
testx -o tap
```

[TAP](https://testanything.org/) is a simple, line-based test reporting protocol. Each test result is one line:

```
TAP version 13
1..3
ok 1 - test_add
ok 2 - test_subtract
not ok 3 - test_divide_by_zero
```

**When to use:** Tools that consume TAP (e.g., `tap-diff`, `tap-spec`, `tap-min`), or when you need a simple, easy-to-parse text format.

**Example — pipe through a TAP formatter:**

```bash
testx -o tap | npx tap-spec
```

---

## Setting the format in config

Instead of passing `-o` every time, set the default format in your `testx.toml`:

```toml
[output]
format = "json"    # pretty | json | junit | tap
```

CLI flags always override the config file, so `testx -o pretty` would still work even with this set.

---

## Comparison

| Format   | Best for                                   | Human-readable | Machine-readable |
| -------- | ------------------------------------------ | -------------- | ---------------- |
| `pretty` | Terminal / local dev                       | Yes            | No               |
| `json`   | Scripts, dashboards, `jq`                  | No             | Yes              |
| `junit`  | CI systems (GitHub Actions, Jenkins, etc.) | No             | Yes              |
| `tap`    | TAP-compatible tools                       | Somewhat       | Yes              |
