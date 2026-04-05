# Test History & Analytics

testx tracks test runs over time and provides analytics to help you identify flaky tests, slow tests, and overall test health.

## Basic usage

```bash
# Quick summary of recent test health
testx history

# Specify a view
testx history <view>
```

## Views

### Summary (default)

```bash
testx history
```

Shows a quick overview: recent pass/fail rates, any flaky tests, and slowest tests.

### Runs

```bash
testx history runs
testx history runs --last 50
```

Table of recent test runs with timestamps, pass/fail counts, and durations.

### Flaky

```bash
testx history flaky
```

Lists tests with pass rates below 95% across recent runs. Helps identify intermittent failures that need attention.

### Slow

```bash
testx history slow
```

Shows the slowest tests trending over recent runs, helping you find performance regressions.

### Health

```bash
testx history health
```

Displays the **Test Health Score** dashboard — a composite score from 0–100 with letter grades (A–F):

| Component     | Weight | What it measures               |
| ------------- | ------ | ------------------------------ |
| Pass Rate     | 50%    | Percentage of tests passing    |
| Stability     | 30%    | Inverse of flakiness           |
| Performance   | 20%    | Duration consistency over time |

Example output:

```
Test Health Score: 87/100 (B+)

  Pass Rate:    95% (47.5/50)
  Stability:    90% (27.0/30)
  Performance:  62% (12.4/20)
```

## Options

| Flag     | Type | Default | Description                             |
| -------- | ---- | ------- | --------------------------------------- |
| `--last` | N    | `20`    | Number of recent runs to analyze        |
| view     | ENUM | `summary` | `summary`, `runs`, `flaky`, `slow`, `health` |

## Storage

Test history is stored in `.testx/` in your project directory (JSON-based, no external database needed). History data is automatically pruned after 90 days by default.

### Configuration

```toml
[history]
enabled = true
max_age_days = 90
```

## Tips

- Run `testx history flaky` before merging PRs to check for intermittent failures
- Use `testx history health` in CI to track test suite quality over time
- Combine with `testx stress` to confirm suspected flaky tests: `testx stress -- --filter test_name`
