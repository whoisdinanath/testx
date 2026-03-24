# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Universal test runner with 11 language adapters
  - Rust (cargo test)
  - Go (go test)
  - Python (pytest)
  - JavaScript/TypeScript (Jest, Vitest, Mocha)
  - Java (Maven Surefire, Gradle)
  - C/C++ (Google Test, CTest)
  - Ruby (RSpec, Minitest)
  - Elixir (ExUnit / mix test)
  - PHP (PHPUnit)
  - .NET (dotnet test)
  - Zig (zig build test)
- Auto-detection of test frameworks from project files
- Multiple output formats: pretty, JSON, JUnit XML, TAP
- Configuration via `testx.toml`
- Timeout support for test runs
- Slowest test reporting
- Plugin system with custom script adapters
- Reporter plugins: Markdown, GitHub Actions, HTML, Desktop notifications
- Watch mode with file system monitoring
- Test filtering with glob patterns
- Retry support for flaky tests
- Parallel test execution across adapters
- Code coverage integration (LCOV, Cobertura, JaCoCo, Go coverage)
- Test history tracking and analytics
- Health score dashboard with flaky test detection
- Shell completions (bash, zsh, fish, PowerShell)

## [0.1.0] - 2024-01-01

### Added

- Initial release
- Core test running functionality
- 11 language adapters
- Pretty terminal output
