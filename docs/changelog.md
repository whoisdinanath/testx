# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
