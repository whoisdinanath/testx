<p align="center">
  <h1 align="center">testx</h1>
  <p align="center"><strong>One command. Any language. Beautiful tests.</strong></p>
</p>

<p align="center">
  <a href="https://github.com/whoisdinanath/testx/actions/workflows/ci.yml"><img src="https://github.com/whoisdinanath/testx/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/whoisdinanath/testx/releases/latest"><img src="https://img.shields.io/github/v/release/whoisdinanath/testx?label=release" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
  <img src="https://img.shields.io/badge/rust-1.87+-orange?logo=rust" alt="Rust 1.87+">
  <img src="https://img.shields.io/badge/languages-11-blueviolet" alt="11 Languages">
  <img src="https://img.shields.io/badge/tests-889-brightgreen" alt="889 Tests">
</p>

**testx** is a universal test runner that auto-detects your project's language and framework, runs your tests, and displays clean, unified output. Zero configuration required.

```
testx · Python (pytest)
────────────────────────────────────────────────────────────

  ✓ tests/test_math.py
    ✓ test_add                                         1ms
    ✓ test_subtract                                    0ms
    ✗ test_divide_by_zero                              1ms

────────────────────────────────────────────────────────────
  FAIL  2 passed, 1 failed (3 total) in 120ms
```

---

## Features

- **Auto-detection** — Identifies language and test framework from project files
- **11 languages** — Rust, Go, Python, JavaScript/TypeScript, Java, C/C++, Ruby, Elixir, PHP, .NET, Zig
- **Multiple output formats** — Pretty (default), JSON, JUnit XML, TAP
- **CI sharding** — Split tests across CI nodes with `--partition slice:1/4` or `hash:2/3`
- **Stress testing** — Run tests N times to find flaky tests with `testx stress`
- **Impact analysis** — Only run tests affected by recent git changes with `--affected`
- **Smart caching** — Skip re-running tests when nothing changed with `--cache`
- **Interactive picker** — Fuzzy-search and pick specific tests with `testx pick`
- **Watch mode** — Re-run tests on file changes
- **Retry logic** — Automatically retry failing tests
- **Parallel execution** — Run multiple test suites concurrently
- **Coverage integration** — LCOV, Cobertura, JaCoCo, Go coverage
- **Plugin system** — Custom adapters, reporter plugins, shell hooks
- **History tracking** — Track test health scores and trends over time

## Supported Languages

| Language | Frameworks | Package Managers |
|----------|-----------|-----------------|
| **Rust** | cargo test | — |
| **Go** | go test | — |
| **Python** | pytest, unittest, Django | uv, poetry, pdm, venv |
| **JavaScript / TypeScript** | Jest, Vitest, Mocha, AVA, Bun | npm, pnpm, yarn, bun |
| **Java / Kotlin** | Maven Surefire, Gradle | mvn, gradle |
| **C / C++** | Google Test, CTest, Meson | cmake, meson |
| **Ruby** | RSpec, Minitest | bundler |
| **Elixir** | ExUnit | mix |
| **PHP** | PHPUnit | composer |
| **C# / .NET / F#** | dotnet test | dotnet |
| **Zig** | zig build test | — |

## Installation

### From source

```bash
cargo install --path .
```

### From releases

Download a prebuilt binary from the [releases page](https://github.com/whoisdinanath/testx/releases).

### Shell completions

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

## Quick Start

```bash
# Run tests in the current directory
testx

# Run tests in a specific project
testx -p /path/to/project

# Detect framework without running tests
testx detect

# Pass extra arguments to the underlying runner
testx -- --filter my_test
```

## Usage

### Output formats

```bash
testx                    # Pretty terminal output (default)
testx -o json            # Machine-readable JSON
testx -o junit > report.xml  # JUnit XML for CI
testx -o tap             # Test Anything Protocol
```

### CI sharding

Split tests across parallel CI jobs:

```bash
# Slice-based (deterministic, ordered)
testx --partition slice:1/4   # Job 1 of 4
testx --partition slice:2/4   # Job 2 of 4

# Hash-based (stable across test additions)
testx --partition hash:1/3
```

**GitHub Actions example:**

```yaml
jobs:
  test:
    strategy:
      matrix:
        shard: [1, 2, 3, 4]
    steps:
      - run: testx --partition slice:${{ matrix.shard }}/4
```

### Flaky test detection

```bash
# Run tests 10 times (default)
testx stress

# Run 50 iterations, stop on first failure
testx stress -n 50 --fail-fast

# Cap total time at 60 seconds
testx stress --max-duration 60
```

Output:

```
Stress Test Report: 10/10 iterations in 5.23s

  Flaky Tests Detected:
    test_network_call (7/10 passed, 70.0% pass rate, avg 12ms)
```

### Impact analysis

Only run tests when relevant source files changed:

```bash
# Skip tests if only docs changed
testx --affected

# Analyze what changed without running
testx impact

# Compare against a branch
testx impact --mode branch:main
```

### Smart caching

```bash
# Skip re-running if source files haven't changed
testx --cache

# Clear the cache
testx cache-clear
```

### Interactive test picker

```bash
testx pick
```

Fuzzy-search through all discovered tests, select one or more, and run only those.

### Other options

```bash
testx --slowest 5        # Show 5 slowest tests
testx --timeout 60       # Kill tests after 60 seconds
testx --raw              # Show raw runner output
testx -v                 # Verbose (show detected command)
```

## Configuration

Create a `testx.toml` in your project root (or run `testx init`):

```toml
# Override detected adapter
# adapter = "python"

# Extra arguments for the test runner
args = ["-v", "--no-header"]

# Timeout in seconds (0 = no timeout)
timeout = 60

# Environment variables
[env]
CI = "true"
```

CLI flags always override config file values.

## Plugin System

### Custom adapters

Define custom test commands in `testx.toml`:

```toml
[[adapters]]
name = "my-framework"
detect = ["my-config.json"]
command = "my-test-runner"
args = ["--reporter", "json"]
output_format = "json"
```

### Reporter plugins

Built-in reporters:

- **Markdown** — Generate markdown test reports
- **GitHub Actions** — Annotations with `::error::` / `::warning::`
- **HTML** — Standalone HTML report
- **Desktop notifications** — System notification on completion

## Building from Source

```bash
git clone https://github.com/whoisdinanath/testx.git
cd testx
cargo build --release
```

### Running the test suite

```bash
cargo test            # Run all tests (889 tests)
cargo clippy          # Lint (0 warnings)
cargo fmt --check     # Format check
```

## Stats

| Metric | Value |
|--------|-------|
| Languages supported | 11 |
| Test frameworks | 20+ |
| Source lines | ~29,000 |
| Test count | 889 (846 unit + 43 integration) |
| Binary size (release) | 2.9 MB |
| Framework detection | ~5ms |
| Rust source files | 53 |
| Dependencies | minimal (clap, serde, colored, toml, anyhow) |
| Clippy warnings | 0 |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[MIT](LICENSE)