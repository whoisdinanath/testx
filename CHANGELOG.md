# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1](https://github.com/whoisdinanath/testx/compare/v0.1.0...v0.1.1) - 2026-04-04

### Added

- publish to crates.io, expand CLI, audit fixes, native file watcher

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