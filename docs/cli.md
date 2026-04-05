# CLI Reference

## Usage

```
testx [OPTIONS] [COMMAND]
```

If no command is given, `run` is used by default.

## Commands

| Command               | Description                             |
| --------------------- | --------------------------------------- |
| `run [-- ARGS]`       | Run tests (default)                     |
| `detect`              | Detect frameworks without running tests |
| `list`                | List all supported adapters             |
| `init`                | Generate a `testx.toml` config file     |
| `completions <SHELL>` | Generate shell completions              |
| `stress`              | Run tests N times to find flaky tests   |
| `impact`              | Analyze test impact from git changes    |
| `pick [-- ARGS]`      | Interactive fuzzy test picker           |
| `cache-clear`         | Clear the smart test cache              |
| `workspace`           | Scan monorepo and run tests across all projects |
| `history`             | Show test history, trends, and flaky analytics  |

## Global options

| Flag          | Short | Type    | Default  | Description                                                                      |
| ------------- | ----- | ------- | -------- | -------------------------------------------------------------------------------- |
| `--path`      | `-p`  | PATH    | `.`      | Project directory                                                                |
| `--output`    | `-o`  | FORMAT  | `pretty` | Output format: `pretty`, `json`, `junit`, `tap`                                  |
| `--slowest`   |       | N       | —        | Show N slowest tests                                                             |
| `--raw`       |       | —       | —        | Show raw test runner output                                                      |
| `--verbose`   | `-v`  | —       | —        | Show detection details and executed commands                                     |
| `--timeout`   | `-t`  | SECONDS | —        | Kill test process after N seconds                                                |
| `--partition` |       | STRING  | —        | CI sharding: `slice:M/N` or `hash:M/N`                                           |
| `--affected`  |       | MODE    | —        | Skip if no git changes. Modes: `head`, `staged`, `branch:<name>`, `commit:<sha>` |
| `--cache`     |       | —       | —        | Skip re-running if nothing changed                                               |
| `--watch`     | `-w`  | —       | —        | Watch mode — re-run tests on file changes                                        |
| `--retries`   |       | N       | —        | Retry failed tests N times before reporting failure                              |
| `--reporter`  |       | STRING  | —        | Activate a reporter plugin: `github`, `markdown`, `html`, `notify`               |

## Stress options

| Flag             | Short | Type    | Default | Description            |
| ---------------- | ----- | ------- | ------- | ---------------------- |
| `-n`, `--count`  | `-n`  | N       | `10`    | Number of iterations   |
| `--fail-fast`    |       | —       | —       | Stop on first failure  |
| `--max-duration` |       | SECONDS | —       | Maximum total duration |

## Impact options

| Flag     | Type | Default | Description                                                  |
| -------- | ---- | ------- | ------------------------------------------------------------ |
| `--mode` | MODE | `head`  | Diff mode: `head`, `staged`, `branch:<name>`, `commit:<sha>` |

## Workspace options

| Flag             | Short | Type    | Default | Description                                                         |
| ---------------- | ----- | ------- | ------- | ------------------------------------------------------------------- |
| `--max-depth`    |       | N       | `5`     | Maximum directory depth to scan (0 = unlimited)                     |
| `--jobs`         | `-j`  | N       | `0`     | Maximum parallel jobs (0 = auto-detect CPUs)                        |
| `--sequential`   |       | —       | —       | Run projects sequentially instead of in parallel                    |
| `--fail-fast`    |       | —       | —       | Stop on first project failure                                       |
| `--filter`       |       | STRING  | —       | Filter to specific languages (comma-separated, e.g., "rust,python") |
| `--include`      |       | STRING  | —       | Include directories normally skipped (e.g., "packages,vendor")      |
| `--list`         |       | —       | —       | Only list discovered projects, don't run tests                      |

## History options

| Flag     | Type | Default   | Description                                             |
| -------- | ---- | --------- | ------------------------------------------------------- |
| `--last` | N    | `20`      | Number of recent runs to analyze                        |
| view     | ENUM | `summary` | View: `summary`, `runs`, `flaky`, `slow`, `health`     |

## Shell completions

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

## Environment variables

| Variable    | Effect                  |
| ----------- | ----------------------- |
| `NO_COLOR`  | Disables colored output |
| `TERM=dumb` | Disables colored output |

## Exit codes

| Code | Meaning                               |
| ---- | ------------------------------------- |
| `0`  | All tests passed                      |
| `1`  | One or more tests failed              |
| `2`  | No framework detected or runner error |
