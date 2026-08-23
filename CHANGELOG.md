# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1] - 2026-08-23

### Changed

- **Release**: tags now publish to crates.io as well as GitHub Releases and npm, and the
  workflow can be dispatched manually so a publish step that failed on an expired registry
  token can be re-run without moving the tag.
- **Dependencies**: `toml` 1.1, `criterion` 0.8 (`criterion::black_box` is deprecated, so the
  benches use `std::hint::black_box`), `notify` 8, and the GitHub Actions used by CI and the
  release workflow.

## [0.3.0] - 2026-08-23

### Added

- **Adapters**: `--adapter <name>` (`-a`) runs one adapter by name instead of auto-detecting,
  and takes precedence over `adapter =` in `testx.toml`.
- **Custom adapters**: an adapter with no detect rules is now opt-in — it never auto-detects
  and only runs when named. This is how a second suite (Playwright e2e, smoke, contract tests)
  lives beside a project's default suite.
- **Custom adapters**: `report_file = "<path>"` parses the report a runner writes to disk
  instead of stdout, for `pytest --junitxml`, `jest-junit`, `gotestsum --junitfile` and
  `PLAYWRIGHT_JUNIT_OUTPUT_NAME`. Falls back to stdout when the file is absent.
- **Workspace**: `testx workspace --exclude <dirs>` to skip directories during the scan.
  `WorkspaceConfig::skip_dirs` was honoured by the scanner but unreachable from the CLI.
- **Adapters**: adapter overrides in `testx.toml` now accept a single alias segment, so
  `adapter = "javascript"` resolves the `JavaScript/TypeScript` adapter (previously only the
  full slashed name worked).

### Fixed

- **JUnit parser**: each `<testsuite>` now owns only its own `<testcase>` elements. Runners
  that emit one suite per file (Playwright, jest-junit, surefire) reported every test once
  per suite, so a 5-test run showed up as 10.
- **JUnit parser**: single-line documents — what `pytest --junitxml` and most JUnit writers
  emit — are parsed instead of falling through to raw output, and `name=` no longer matches
  the tail of `classname=`.
- **Custom adapters**: an adapter with an empty `detect` matched every directory, because the
  empty detect file joined to the project directory itself.
- **Custom adapters**: a `detect` block with only `commands`, `env` or `content` rules never
  matched; the file check vetoed it before the other rules ran.
- **Exit code**: a runner that exits non-zero while every reported test passes is now a
  failure. Coverage thresholds (`pytest --cov-fail-under`), warnings-as-errors, plugin and
  collection errors, and timeouts were silently reported as success.
- **Python**: Accumulate pytest failures and collection errors in summary-only output instead of
  letting the final category overwrite earlier failures.
- **Workspace**: `testx workspace --list` printed a blank name for the root project.
- **MSRV**: `package.rust-version` is now declared and verified in CI. The documented
  minimum was 1.87, but the crate has required 1.91 since it started using
  `str::ceil_char_boundary`; 1.87–1.90 fail to compile.
- **Lints**: resolved six `clippy` errors (`unnecessary_sort_by`, `question_mark`) that fail
  the `lint` job on Rust 1.98.

### Changed

- **CI**: least-privilege `permissions`, PR concurrency cancellation, `RUSTFLAGS` scoped per
  job (it previously applied to `cargo install`, so a warning in a third-party crate could
  fail the audit/coverage jobs), prebuilt `cargo-audit`/`cargo-tarpaulin` via
  `taiki-e/install-action`, a real MSRV job, and no dependency on `bc` (absent from
  `ubuntu-24.04` runners).
- **Release**: tag pushes now run fmt/clippy/tests and assert the tag matches
  `Cargo.toml` before any artifact is published; `contents: write` is scoped to the release
  job; `musl-tools` install runs `apt-get update` first.
- **Dependabot**: added for cargo, GitHub Actions, and the npm distribution package.

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
  - Full detect + parse 100 tests: ~138–173 µs per language
  - Config loading: ~1.4 µs (no file) / ~18 µs (with testx.toml)
  - Parse scaling: 10 tests → 7 µs, 1000 tests → 570 µs, 5000 tests → 3 ms
  - JSON serialization: 1000 tests → 422 µs
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
