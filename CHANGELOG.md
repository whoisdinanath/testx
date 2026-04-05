# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-04-05

### Added

- **Workspace / monorepo support** (`testx workspace`) — scan directory tree, discover projects, and run tests across all of them in parallel
  - `--max-depth` — control scan depth (default: 5)
  - `--jobs` / `--sequential` — parallel or serial execution
  - `--fail-fast` — stop on first project failure
  - `--filter` — filter by language (e.g., `rust,python`)
  - `--include` — override default skipped directories (e.g., `packages/`, `vendor/`)
  - `--list` — list discovered projects without running tests
- **Test history & analytics** (`testx history`) — track test runs and analyze trends
  - 5 views: `summary`, `runs`, `flaky`, `slow`, `health`
  - Health Score dashboard (0–100, A–F grading) based on pass rate, stability, and performance
  - Flaky test detection (pass rate < 95%)
  - Slowest test trending across runs

### Fixed

- **Watch mode**: File change detection was broken (poll always returned empty); now properly drains watcher channel
- **macOS notifications**: AppleScript command injection — fixed backslash and quote escaping
- **Windows notifications**: PowerShell/XML injection — added `"` and `'` entity escaping, switched to here-string syntax
- **Markdown reporter**: Test names with `|`, `<>`, `[]` now properly escaped in tables
- **Retry filter**: Rust adapter used `--exact` with regex-OR patterns, causing retries to run zero tests
- **Timeout config**: `timeout = 0` in `testx.toml` now means "no timeout" instead of immediately killing tests
- **Color output**: `CI` environment variable no longer disables colors — only `NO_COLOR` and `TERM=dumb` do
- **Script adapter**: Path traversal via `working_dir` — now rejects absolute and `..`-containing paths

### Security

- **Script adapter**: Reject absolute paths and parent-directory traversal in plugin `working_dir` config
- **Timeout handler**: Thread panic results now logged as warnings instead of silently swallowed

### Changed

- Updated documentation with workspace and history guides
- Updated CLI reference with missing global options (`--watch`, `--retries`, `--reporter`)

## [0.1.0] - 2026-04-04

### Added

- **Universal test runner** with auto-detection of language and framework
- **11 language adapters**
  - Rust (cargo test)
  - Go (go test)
  - Python (pytest, unittest, Django) with uv/poetry/pdm/venv support
  - JavaScript/TypeScript (Jest, Vitest, Mocha, AVA, Bun) with npm/pnpm/yarn/bun
  - Java/Kotlin (Maven Surefire, Gradle)
  - C/C++ (Google Test, CTest, Meson)
  - Ruby (RSpec, Minitest)
  - Elixir (ExUnit)
  - PHP (PHPUnit)
  - C#/.NET/F# (dotnet test)
  - Zig (zig build test)
- **Output formats**: pretty, JSON, JUnit XML, TAP
- **CI sharding** with `--partition slice:N/M` and `hash:N/M`
- **Stress testing** (`testx stress`) — run tests N times to detect flaky tests
- **Impact analysis** (`testx impact`, `--affected`) — git-based test relevance detection
- **Smart caching** (`--cache`) — skip re-running when source files haven't changed
- **Interactive test picker** (`testx pick`) — fuzzy search and select tests to run
- **Watch mode** with file system monitoring
- **Retry logic** for flaky tests
- **Parallel execution** across adapters
- **Coverage integration** (LCOV, Cobertura, JaCoCo, Go coverage)
- **Plugin system** — custom script adapters and reporter plugins
- **Reporter plugins**: Markdown, GitHub Actions, HTML, Desktop notifications
- **Test history tracking** with health score dashboard
- **Shell completions** for bash, zsh, fish, PowerShell
- **Configuration** via `testx.toml` with `testx init` scaffolding
- **Timeout support** for test runs
- **Slowest test reporting**
