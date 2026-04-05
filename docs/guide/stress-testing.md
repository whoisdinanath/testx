# Flaky Test Detection

Find intermittently failing tests by running your suite multiple times.

## Usage

```bash
testx stress
testx stress -n 20
testx stress --fail-fast
testx stress --max-duration 300
testx stress -n 30 --threshold 0.9
```

## Options

| Flag                | Default | Description                                                    |
| ------------------- | ------- | -------------------------------------------------------------- |
| `-n`, `--count`     | `10`    | Number of iterations                                           |
| `--fail-fast`       | off     | Stop on first failure                                          |
| `--max-duration`    | none    | Maximum total seconds; stops early if exceeded                 |
| `--threshold`       | none    | Minimum pass rate (0.0–1.0). Exit 1 if any flaky test is below |
| `--parallel-stress` | `0`     | Number of parallel stress workers (0 = sequential)             |
| `-- [ARGS]`         | —       | Extra args passed to the test runner                           |

## Output

Each iteration prints a summary line:

```
▸ Iteration 1/10... PASS (42.1ms)
▸ Iteration 2/10... PASS (38.7ms)
▸ Iteration 3/10... FAIL (41.2ms, 1 failed)
```

After all iterations, testx generates a **stress report**:

- Total iterations completed vs requested
- Total duration and whether it stopped early
- Which iterations failed and which tests failed in each
- **Timing statistics** — mean, median, std dev, P95, P99, coefficient of variation (CV)
- **Flaky tests** — tests that both passed and failed across iterations, with severity classification

## Severity levels

Each flaky test is classified by its pass rate:

| Severity     | Icon | Pass Rate | Meaning                                      |
| ------------ | ---- | --------- | -------------------------------------------- |
| **Critical** | 🔴   | < 50%     | Almost always fails — likely a real bug      |
| **High**     | 🟠   | 50–80%    | Frequently flaky                             |
| **Medium**   | 🟡   | 80–95%    | Occasionally flaky                           |
| **Low**      | 🟢   | > 95%     | Rarely flaky, possibly environment-dependent |

## Flaky test report

A test is considered flaky if it passed in some iterations and failed in others. For each flaky test, testx reports:

- **Severity level** (Critical / High / Medium / Low)
- **Pass rate** (e.g., 80% — passed 8 out of 10 runs)
- **Wilson score lower bound** — statistical confidence interval for the true pass rate
- **Average duration** and **timing CV** (coefficient of variation)

Example output:

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

## Examples

Run 50 iterations, stop if total time exceeds 5 minutes:

```bash
testx stress -n 50 --max-duration 300
```

Run stress tests with extra pytest flags:

```bash
testx stress -n 10 -- -x --tb=short
```

Enforce a minimum pass rate (useful in CI):

```bash
testx stress -n 20 --threshold 0.95
```

## Exit codes

| Condition                              | Exit | Error message                                        |
| -------------------------------------- | ---- | ---------------------------------------------------- |
| All iterations passed                  | `0`  | —                                                    |
| Flaky tests detected                   | `1`  | `flaky tests detected (N flaky across M iterations)` |
| Tests failing consistently (not flaky) | `1`  | `stress test failed — tests failing consistently`    |
| Threshold not met                      | `1`  | `stress test threshold not met`                      |

With `--fail-fast`, the stress test stops after the first iteration with any failure and reports "(stopped early)".
