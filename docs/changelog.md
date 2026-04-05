# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-04-05

### Added

- **Custom adapter system** — define custom test adapters in `testx.toml` or global config
  - `[[custom_adapter]]` config section with `name`, `detect`, `command`, `args`, `output`, `confidence`, `check`, `working_dir`, `env`
  - Flexible `detect` config: accepts a simple string (`detect = "BUILD"`) or a full table with `files`, `commands`, `env`, `content`, and `search_depth`
  - Content-based detection: match file contents (e.g., check if `Makefile` contains `test:`)
  - Command-based detection: verify commands succeed (exit 0) before activating adapter
  - Environment variable detection: require specific env vars to be set
  - `check` field to verify the test runner is installed before execution
  - Custom adapters participate in confidence-based detection alongside built-in adapters
- **Global adapter definitions** — load adapters from `~/.config/testx/adapters/*.toml`
  - Supports both single adapter files and files with `[[custom_adapter]]` arrays
  - XDG_CONFIG_HOME respected, cross-platform home directory resolution
- **`testx adapters` subcommand** — list built-in, project-local, and global custom adapters with metadata
- **`--no-custom-adapters` CLI flag** — disable custom adapter loading for security or debugging
- **Backward-compatible config** — `parse` field (v0.1.x) still works as alias for `output`
- **Stress test improvements** — severity classification (Critical/High/Medium/Low), timing statistics (mean, median, P95, P99, CV), Wilson score bounds, `--threshold` and `--parallel-stress` flags, improved exit messages
- **npm distribution** — `npm install -g @whoisdinanath/testx` with automatic platform binary download
- **Install script** — `curl -fsSL .../install.sh | sh` one-liner for macOS/Linux
- **Overhead benchmarks** (`benches/overhead.rs`) — full pipeline benchmarks measuring detection + parsing overhead
- **CI binary size guard** — Fails CI if release binary exceeds 4 MB
- **CI benchmark check** — Benchmark compilation verified on every push
- **Performance section in README** — Published overhead numbers with reproducible benchmark command

### Changed

- **Binary size**: 3.8 MB → 2.2 MB (42% reduction) via `opt-level = "z"`, `lto = "thin"`, `strip = true`, `panic = "abort"`, `codegen-units = 1`
- **Installation docs**: README now lists 4 install methods (crates.io, npm, install script, source)
- **Release workflow**: Added npm publish job
- **Framework detection stat**: Updated from "~5ms" to "< 200 µs" based on actual benchmarks
- **Detection engine**: added `register()` method for dynamic adapter registration

### Fixed

- **Cross-platform**: Fixed literal `~` in path fallback for global config directory (bug on all platforms)
- **Cross-platform**: Added `HOMEDRIVE`+`HOMEPATH` fallback for Windows service account home directories
- **Windows**: Custom adapter commands now use `cmd /C` wrapper to find `.cmd`/`.bat` scripts (npm, yarn, gradle, etc.)
- **Windows**: `glob_detect()` now strips both `/` and `\` path separators

## [0.1.1] - 2026-04-05

### Added

- **Workspace / monorepo support** (`testx workspace`) — scan directory tree, discover projects, and run tests across all of them in parallel
  - Options: `--max-depth`, `--jobs`, `--sequential`, `--fail-fast`, `--filter`, `--include`, `--list`
- **Test history & analytics** (`testx history`) — track test runs and analyze trends
  - 5 views: `summary`, `runs`, `flaky`, `slow`, `health`
  - Health Score dashboard (0–100, A–F grading)

### Fixed

- **Watch mode**: File change detection was broken; now properly drains watcher channel
- **macOS notifications**: AppleScript command injection — fixed escaping
- **Windows notifications**: PowerShell/XML injection — added entity escaping
- **Markdown reporter**: Test names with `|`, `<>`, `[]` now properly escaped in tables
- **Retry filter**: Rust adapter used `--exact` with regex-OR patterns, causing retries to run zero tests
- **Timeout config**: `timeout = 0` now means "no timeout" instead of immediately killing tests
- **Color output**: `CI` environment variable no longer disables colors — only `NO_COLOR` and `TERM=dumb` do

### Changed

- Updated CLI documentation with workspace, history commands, and missing global options

## [0.1.0] - 2026-04-04

### Added

- **Universal test runner** with auto-detection of language and framework
- **11 language adapters**: Rust, Go, Python, JavaScript/TypeScript, Java/Kotlin, C/C++, Ruby, Elixir, PHP, .NET, Zig
- **Output formats**: pretty, JSON, JUnit XML, TAP
- **CI sharding** with `--partition slice:N/M` and `hash:N/M`
- **Stress testing** (`testx stress`) — run tests N times to detect flaky tests
- **Impact analysis** (`testx impact`, `--affected`) — git-based test relevance
- **Smart caching** (`--cache`) — skip re-running when nothing changed
- **Interactive test picker** (`testx pick`) — fuzzy search and select tests
- **Watch mode** with file system monitoring
- **Retry logic** for flaky tests
- **Parallel execution** across adapters
- **Coverage integration** (LCOV, Cobertura, JaCoCo, Go)
- **Plugin system** — custom adapters and reporter plugins (Markdown, GitHub Actions, HTML, notifications)
- **Test history tracking** with health scores
- **Shell completions** (bash, zsh, fish, PowerShell)
- **Configuration** via `testx.toml` with `testx init`
