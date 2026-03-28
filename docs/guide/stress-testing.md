# Flaky Test Detection

Find intermittently failing tests by running your suite multiple times.

## Usage

```bash
testx stress
testx stress -n 20
testx stress --fail-fast
testx stress --max-duration 300
```

## Options

| Flag             | Default | Description                                    |
| ---------------- | ------- | ---------------------------------------------- |
| `-n`, `--count`  | `10`    | Number of iterations                           |
| `--fail-fast`    | off     | Stop on first failure                          |
| `--max-duration` | none    | Maximum total seconds; stops early if exceeded |
| `-- [ARGS]`      | —       | Extra args passed to the test runner           |

## Output

Each iteration prints a summary line:

```
▸ Iteration 1/10... PASS (42.1ms)
▸ Iteration 2/10... PASS (38.7ms)
▸ Iteration 3/10... FAIL (41.2ms, 1 failed)
```

After all iterations, testx generates a **stress report**:

- Total iterations completed vs requested
- Total duration
- Which iterations failed and which tests failed in each
- **Flaky tests** — tests that both passed and failed across iterations

## Flaky test report

A test is considered flaky if it passed in some iterations and failed in others. For each flaky test, testx reports:

- **Pass rate** (e.g., 80% — passed 8 out of 10 runs)
- **Average / min / max duration**
- Suite name

## Examples

Run 50 iterations, stop if total time exceeds 5 minutes:

```bash
testx stress -n 50 --max-duration 300
```

Run stress tests with extra pytest flags:

```bash
testx stress -n 10 -- -x --tb=short
```

## Exit code

Exits with code `1` if any iteration had failures, `0` if all passed.
