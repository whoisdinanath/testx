# CLI Reference

This is a complete reference for all testx commands, flags, and options.

---

## Usage

```
testx [OPTIONS] [COMMAND] [-- ARGS]
```

If no command is given, testx defaults to `run` — it detects your test framework and runs your tests.

**Quick examples:**

```bash
testx                          # Run tests (auto-detect everything)
testx detect                   # Show what was detected, don't run
testx -o json                  # Run tests, output as JSON
testx -- -k "test_login"      # Pass args to the underlying test runner
testx -w                       # Watch mode — re-run on file changes
```

---

## Commands

| Command               | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| `run [-- ARGS]`       | Run tests (this is the default when no command is specified)                |
| `detect`              | Show detected language and framework without running tests                 |
| `list`                | List all supported built-in adapters                                       |
| `adapters`            | List all adapters including custom ones from `testx.toml` and global config |
| `init`                | Generate a `testx.toml` config file with detected defaults                 |
| `completions <SHELL>` | Generate tab-completion scripts (`bash`, `zsh`, `fish`, `powershell`)      |
| `stress`              | Run tests N times to detect flaky (intermittently failing) tests           |
| `impact`              | Analyze which tests are affected by recent git changes                     |
| `pick [-- ARGS]`      | Interactive fuzzy picker — search and select tests to run                  |
| `cache-clear`         | Clear the smart test cache (forces a full re-run next time)                |
| `workspace`           | Discover and run tests across all projects in a monorepo                   |
| `history`             | Show test history, trends, flaky reports, and health scores                |

---

## Global options

These flags work with any command.

| Flag                   | Short | Type    | Default  | Description                                                                      |
| ---------------------- | ----- | ------- | -------- | -------------------------------------------------------------------------------- |
| `--path`               | `-p`  | PATH    | `.`      | Project directory to run in                                                      |
| `--output`             | `-o`  | FORMAT  | `pretty` | Output format: `pretty`, `json`, `junit`, `tap`                                  |
| `--slowest`            |       | N       | —        | Show N slowest tests at the end of the run                                       |
| `--raw`                |       | —       | —        | Show raw output from the test runner (no formatting)                             |
| `--verbose`            | `-v`  | —       | —        | Show detection details and the exact command being executed                       |
| `--timeout`            | `-t`  | SECONDS | —        | Kill the test process after N seconds                                            |
| `--partition`          |       | STRING  | —        | CI sharding: `slice:M/N` or `hash:M/N` (see [Sharding](guide/sharding.md))       |
| `--affected`           |       | MODE    | —        | Skip if no git changes. Modes: `head`, `staged`, `branch:<name>`, `commit:<sha>` |
| `--cache`              |       | —       | —        | Skip re-running unchanged tests (see [Caching](guide/caching.md))                |
| `--watch`              | `-w`  | —       | —        | Watch mode — re-run tests automatically when files change                        |
| `--retries`            |       | N       | —        | Retry failed tests up to N times before reporting failure                        |
| `--reporter`           |       | STRING  | —        | Activate a reporter plugin: `github`, `markdown`, `html`, `notify`               |
| `--no-custom-adapters` |       | —       | —        | Disable custom adapters from `testx.toml` and global config                      |
| `--jobs`               | `-j`  | N       | —        | Number of parallel jobs (0 = auto-detect CPU count)                              |

---

## Run options

These flags are specific to the `run` command (the default).

| Flag          | Short | Type    | Default | Description                                                     |
| ------------- | ----- | ------- | ------- | --------------------------------------------------------------- |
| `--filter`    | `-f`  | PATTERN | —       | Only run tests whose names match this pattern (glob: `*foo*`)   |
| `--exclude`   |       | PATTERN | —       | Skip tests whose names match this pattern                       |
| `--fail-fast` |       | —       | —       | Stop on first failure                                           |
| `--coverage`  |       | —       | —       | Collect code coverage data during the run                       |
| `-- [ARGS]`   |       | —       | —       | Everything after `--` is passed directly to the test runner     |

**Examples:**

```bash
# Only run tests containing "auth"
testx -f "*auth*"

# Stop as soon as something fails
testx --fail-fast

# Pass pytest-specific flags
testx -- -x --tb=short -v

# Collect coverage
testx --coverage
```

---

## Stress options

The `stress` command runs your tests multiple times to find flaky (non-deterministic) tests.

| Flag                | Short | Type    | Default | Description                                                         |
| ------------------- | ----- | ------- | ------- | ------------------------------------------------------------------- |
| `-n`, `--count`     |       | N       | `10`    | Number of iterations to run                                         |
| `--fail-fast`       |       | —       | —       | Stop on first failure                                               |
| `--max-duration`    |       | SECONDS | —       | Maximum total duration across all iterations                        |
| `--threshold`       |       | FLOAT   | —       | Minimum pass rate (0.0–1.0). Exit 1 if any test is below this       |
| `--parallel-stress` |       | N       | `0`     | Number of parallel stress workers (0 = run sequentially)            |
| `-- [ARGS]`         |       | —       | —       | Extra args passed to the test runner                                |

**Examples:**

```bash
# Run tests 50 times to find flaky failures
testx stress -n 50

# Run 100 times, 4 iterations in parallel, stop on first fail
testx stress -n 100 --parallel-stress 4 --fail-fast

# Require 95% pass rate
testx stress -n 20 --threshold 0.95
```

---

## Impact options

The `impact` command shows which tests are affected by recent git changes.

| Flag     | Type | Default | Description                                                  |
| -------- | ---- | ------- | ------------------------------------------------------------ |
| `--mode` | MODE | `head`  | Diff mode: `head`, `staged`, `branch:<name>`, `commit:<sha>` |

**Examples:**

```bash
# Tests affected by uncommitted changes
testx impact

# Tests affected by staged changes only
testx impact --mode staged

# Tests affected since branching off from main
testx impact --mode branch:main
```

---

## Workspace options

The `workspace` command discovers and tests all projects in a monorepo.

| Flag           | Short | Type   | Default | Description                                                         |
| -------------- | ----- | ------ | ------- | ------------------------------------------------------------------- |
| `--max-depth`  |       | N      | `5`     | Maximum directory depth to scan                                     |
| `--jobs`       | `-j`  | N      | `0`     | Parallel jobs (0 = auto-detect CPU count)                           |
| `--sequential` |       | —      | —       | Run projects one at a time instead of in parallel                   |
| `--fail-fast`  |       | —      | —       | Stop on first project failure                                       |
| `--filter`     |       | STRING | —       | Filter to specific languages (comma-separated, e.g., `rust,python`) |
| `--include`    |       | STRING | —       | Include directories normally skipped (e.g., `packages,vendor`)      |
| `--list`       |       | —      | —       | Only list discovered projects — don't run tests                     |

**Examples:**

```bash
# Test everything in the monorepo
testx workspace

# List what testx would test (dry run)
testx workspace --list

# Only test Rust and Python projects
testx workspace --filter rust,python

# Run one project at a time (useful for debugging)
testx workspace --sequential
```

---

## History options

The `history` command shows test analytics over time.

| Flag     | Type | Default   | Description                                                |
| -------- | ---- | --------- | ---------------------------------------------------------- |
| `--last` | N    | `20`      | Number of recent runs to include in the analysis           |
| view     | ENUM | `summary` | View mode: `summary`, `runs`, `flaky`, `slow`, or `health` |

**Examples:**

```bash
# Quick summary of recent test activity
testx history

# See individual run results
testx history runs

# Find tests that sometimes pass and sometimes fail
testx history flaky

# Get a health score for your test suite
testx history health
```

---

## Shell completions

Generate tab-completion scripts so you can press Tab to auto-complete testx commands and flags:

```bash
# Bash
testx completions bash > ~/.local/share/bash-completion/completions/testx

# Zsh
testx completions zsh > ~/.local/share/zsh/site-functions/_testx

# Fish
testx completions fish > ~/.config/fish/completions/testx.fish

# PowerShell
testx completions powershell >> $PROFILE
```

Restart your shell after generating completions.

---

## Environment variables

| Variable    | Effect                                       |
| ----------- | -------------------------------------------- |
| `NO_COLOR`  | Disables colored output (respects convention) |
| `TERM=dumb` | Disables colored output                       |

---

## Exit codes

| Code | Meaning                               |
| ---- | ------------------------------------- |
| `0`  | All tests passed                      |
| `1`  | One or more tests failed              |
| `2`  | No framework detected or runner error |
