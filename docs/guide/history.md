# Test History & Analytics

testx automatically tracks your test runs over time. This lets you spot trends, identify flaky tests, find performance regressions, and monitor the overall health of your test suite.

History data is stored locally in your project's `.testx/` directory — no external database or service needed.

---

## Basic usage

```bash
# Quick summary of recent test activity
testx history
```

You can also specify a view to see different aspects of your test history:

```bash
testx history <view>
```

Available views: `summary`, `runs`, `flaky`, `slow`, `health`

---

## Views

### Summary (default)

```bash
testx history
```

A quick overview showing recent pass/fail rates, any flaky tests, and the slowest tests. This is the best starting point when you want a snapshot of your test suite's state.

### Runs

```bash
testx history runs
testx history runs --last 50
```

A table of individual test runs with timestamps, pass/fail/skip counts, and durations. Useful for seeing the timeline of results — did tests start failing at a particular point?

### Flaky

```bash
testx history flaky
```

Lists tests with pass rates below 95% across recent runs. A "flaky" test is one that sometimes passes and sometimes fails without code changes. These are particularly harmful because they erode trust in the test suite and slow down CI.

**When to use:** Run this before merging PRs to check for intermittent failures. If a test shows up here, consider using `testx stress` to confirm it's genuinely flaky.

### Slow

```bash
testx history slow
```

Shows the slowest tests trending over recent runs. Useful for catching performance regressions — if a test that used to take 50ms is now taking 500ms, it'll show up here.

### Health

```bash
testx history health
```

Displays the **Test Health Score** — a composite score from 0 to 100 with a letter grade (A through F). This gives you a single number to track test suite quality over time.

The score is computed from three components:

| Component   | Weight | What it measures                                          |
| ----------- | ------ | --------------------------------------------------------- |
| Pass Rate   | 50%    | What percentage of tests are passing?                     |
| Stability   | 30%    | How consistent are test results? (inverse of flakiness)   |
| Performance | 20%    | Are test durations stable or getting worse over time?     |

Example output:

```
Test Health Score: 87/100 (B+)

  Pass Rate:    95% (47.5/50)
  Stability:    90% (27.0/30)
  Performance:  62% (12.4/20)
```

**Tip:** Use `testx history health` in CI to track your score over time. If the score drops, investigate with `testx history flaky` and `testx history slow` to find the cause.

---

## Options

| Flag     | Type | Default   | Description                                          |
| -------- | ---- | --------- | ---------------------------------------------------- |
| `--last` | N    | `20`      | Number of recent test runs to include in the analysis |
| view     | ENUM | `summary` | `summary`, `runs`, `flaky`, `slow`, `health`          |

---

## Storage and configuration

Test history is stored as JSON files in `.testx/` in your project directory. No external database is needed.

History data is automatically pruned after 90 days by default. You can change this in `testx.toml`:

```toml
[history]
enabled = true          # Set to false to disable history tracking entirely
max_age_days = 90       # Days to keep history before pruning
```

!!! tip "Add `.testx/` to `.gitignore`"
    The `.testx/` directory contains local cache and history data. You'll generally want to add it to `.gitignore` since it's machine-specific.

---

## Practical tips

- **Before merging a PR:** Run `testx history flaky` to check if any tests have been intermittently failing
- **Performance monitoring:** Run `testx history slow` after major refactors to catch slowdowns early
- **CI quality gate:** Use `testx history health` to set a quality threshold — fail the build if the health score drops below a certain level
- **Confirming flakiness:** If a test appears in `testx history flaky`, use `testx stress -- --filter <test_name>` to reproduce the flaky behavior
