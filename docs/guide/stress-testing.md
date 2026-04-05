# Flaky Test Detection

A **flaky test** is a test that sometimes passes and sometimes fails, even when the code hasn't changed. Flaky tests are one of the biggest productivity killers in software development — they erode trust in your test suite and cause developers to ignore failures.

testx's `stress` command helps you find flaky tests by running your suite multiple times and tracking which tests have inconsistent results.

---

## Quick start

```bash
# Run tests 10 times (default)
testx stress

# Run tests 50 times for more confidence
testx stress -n 50

# Stop on the first failure
testx stress --fail-fast
```

---

## How it works

1. testx runs your full test suite N times (default: 10)
2. It tracks which tests pass and fail in **each** iteration
3. After all iterations, it generates a **stress report** identifying tests that were inconsistent

A test is considered **flaky** if it passed in some iterations and failed in others.

---

## Options

| Flag                | Default | Description                                                      |
| ------------------- | ------- | ---------------------------------------------------------------- |
| `-n`, `--count`     | `10`    | Number of times to run the suite                                 |
| `--fail-fast`       | off     | Stop on the first iteration that has any failure                 |
| `--max-duration`    | none    | Maximum total time in seconds (stops early if exceeded)          |
| `--threshold`       | none    | Minimum pass rate (0.0–1.0). Exit 1 if any test is below this   |
| `--parallel-stress` | `0`     | Run N iterations in parallel (0 = run sequentially)              |
| `-- [ARGS]`         | —       | Extra args passed directly to the test runner                    |

---

## Understanding the output

### Iteration progress

Each iteration prints a summary line as it completes:

```
▸ Iteration 1/10... PASS (42.1ms)
▸ Iteration 2/10... PASS (38.7ms)
▸ Iteration 3/10... FAIL (41.2ms, 1 failed)
```

### Stress report

After all iterations, testx generates a comprehensive report:

```
Stress Test Report: 20/20 iterations in 15.16s

  Flaky tests detected (4):
    🔴 [CRITICAL] timing_lock_acquisition (6/20 passed, 30.0%, wilson≥14.5%)
    🟠 [HIGH]     network_timeout (12/20 passed, 60.0%, wilson≥38.7%)
    🟡 [MEDIUM]   temp_dir_collision (16/20 passed, 80.0%, wilson≥58.4%)
    🟢 [LOW]      rare_gc_pause (49/50 passed, 98.0%, wilson≥89.5%)

  Timing Statistics:
    Mean: 757.8ms | Median: 753.2ms | Std Dev: 89.9ms
    P95: 892.0ms | P99: 906.8ms | CV: 0.12
```

The report includes:

- **Iteration summary** — How many completed vs requested, and total duration
- **Flaky tests** — Tests that had inconsistent results, with severity
- **Timing statistics** — Mean, median, standard deviation, P95, P99, and coefficient of variation (CV)

---

## Severity levels

Each flaky test is classified by its pass rate:

| Severity     | Icon | Pass Rate | What it means                                          |
| ------------ | ---- | --------- | ------------------------------------------------------ |
| **Critical** | 🔴   | < 50%     | Fails more often than it passes — likely a real bug   |
| **High**     | 🟠   | 50–80%    | Frequently flaky — needs urgent attention              |
| **Medium**   | 🟡   | 80–95%    | Occasionally flaky — annoying but less urgent          |
| **Low**      | 🟢   | > 95%     | Rarely flaky — may be environment-dependent            |

### What's the Wilson score?

The **Wilson score lower bound** is a statistical measure that gives a confidence interval for the true pass rate. It accounts for sample size — 1 failure in 10 runs is less concerning than 10 failures in 100 runs, even though both are 90%.

The Wilson score tells you: "we're 95% confident the true pass rate is at least this value."

---

## Practical examples

### Find flaky tests before a release

```bash
testx stress -n 50
```

Run 50 iterations to build high confidence. If nothing is flagged, your suite is likely stable.

### Limit total runtime

```bash
testx stress -n 100 --max-duration 300
```

Run up to 100 iterations, but stop after 5 minutes regardless. The report covers however many iterations completed.

### Set a quality threshold in CI

```bash
testx stress -n 20 --threshold 0.95
```

Exit with code 1 if any test has a pass rate below 95%. This is a quality gate — the build fails if flaky tests are detected.

### Run iterations in parallel

```bash
testx stress -n 100 --parallel-stress 4
```

Run 4 iterations at the same time for faster results. Note: parallel execution may itself introduce flakiness (resource contention), so use this for speed when you're confident your tests are thread-safe.

### Stress with extra test runner flags

```bash
testx stress -n 10 -- -x --tb=short
```

Everything after `--` goes to the underlying test runner.

---

## Exit codes

| Condition                              | Exit Code | Message                                                       |
| -------------------------------------- | --------- | ------------------------------------------------------------- |
| All iterations passed                  | `0`       | —                                                             |
| Flaky tests detected                   | `1`       | `flaky tests detected (N flaky across M iterations)`          |
| Tests failing consistently (not flaky) | `1`       | `stress test failed — tests failing consistently`             |
| Threshold not met                      | `1`       | `stress test threshold not met`                                |

With `--fail-fast`, the stress test stops after the first iteration with any failure and reports "(stopped early)".
