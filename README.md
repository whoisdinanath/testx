<p align="center">
  <h1 align="center">testx</h1>
  <p align="center"><strong>One command. Any language. Beautiful tests.</strong></p>
</p>

<p align="center">
  <a href="https://crates.io/crates/testx-cli"><img src="https://img.shields.io/crates/v/testx-cli" alt="crates.io"></a>
  <a href="https://crates.io/crates/testx-cli"><img src="https://img.shields.io/crates/d/testx-cli" alt="Downloads"></a>
  <a href="https://github.com/whoisdinanath/testx/actions/workflows/ci.yml"><img src="https://github.com/whoisdinanath/testx/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/whoisdinanath/testx/releases/latest"><img src="https://img.shields.io/github/v/release/whoisdinanath/testx?label=release" alt="Release"></a>
  <a href="https://testx-cli.readthedocs.io/"><img src="https://img.shields.io/badge/docs-readthedocs-blue?logo=readthedocs" alt="Docs"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License"></a>
  <img src="https://img.shields.io/badge/rust-1.87+-orange?logo=rust" alt="Rust 1.87+">
  <img src="https://img.shields.io/badge/languages-11-blueviolet" alt="11 Languages">
  <img src="https://img.shields.io/badge/tests-1150-brightgreen" alt="1150 Tests">
</p>

**testx** is a universal test runner that auto-detects your project's language and framework, runs your tests, and displays clean, unified output. Zero configuration required.

<p align="center">
  <img src="demo/demo.gif" alt="testx demo" width="800">
</p>

## Why testx?

|                     | Without testx                 | With testx              |
| ------------------- | ----------------------------- | ----------------------- |
| **Rust**            | `cargo test`                  | `testx`                 |
| **Python**          | `uv run pytest -v`            | `testx`                 |
| **Go**              | `go test -v ./...`            | `testx`                 |
| **JavaScript**      | `npx vitest run` / `npx jest` | `testx`                 |
| **Java**            | `mvn test` / `gradle test`    | `testx`                 |
| **Output**          | Different per language        | Unified, beautiful      |
| **CI sharding**     | Manual config                 | `--partition slice:1/4` |
| **Flaky detection** | Custom scripts                | `testx stress`          |

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
- **Plugin system** — Custom adapters (project-local and global), reporter plugins, shell hooks
- **History tracking** — Track test health scores, flaky tests, and trends over time
- **Monorepo support** — Scan and test all projects in a workspace with `testx workspace`

## Supported Languages

| Language                    | Frameworks                    | Package Managers      |
| --------------------------- | ----------------------------- | --------------------- |
| **Rust**                    | cargo test                    | —                     |
| **Go**                      | go test                       | —                     |
| **Python**                  | pytest, unittest, Django      | uv, poetry, pdm, venv |
| **JavaScript / TypeScript** | Jest, Vitest, Mocha, AVA, Bun | npm, pnpm, yarn, bun  |
| **Java / Kotlin**           | Maven Surefire, Gradle        | mvn, gradle           |
| **C / C++**                 | Google Test, CTest, Meson     | cmake, meson          |
| **Ruby**                    | RSpec, Minitest               | bundler               |
| **Elixir**                  | ExUnit                        | mix                   |
| **PHP**                     | PHPUnit                       | composer              |
| **C# / .NET / F#**          | dotnet test                   | dotnet                |
| **Zig**                     | zig build test                | —                     |

## Installation

### From crates.io (recommended)

```bash
cargo install testx-cli
```

### npm

```bash
npm install -g @whoisdinanath/testx
```

### Install script (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/whoisdinanath/testx/main/install.sh | sh
```

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

# Require all tests to have ≥90% pass rate
testx stress -n 20 --threshold 0.9
```

Output includes severity classification and timing statistics:

```
Stress Test Report: 20/20 iterations in 15.16s

  Flaky tests detected (4):
    🔴 [CRITICAL] timing_lock_acquisition (6/20 passed, 30.0% pass rate)
    🟠 [HIGH]     network_timeout (12/20 passed, 60.0% pass rate)
    🟡 [MEDIUM]   temp_dir_collision (16/20 passed, 80.0% pass rate)
    🟢 [LOW]      rare_gc_pause (49/50 passed, 98.0% pass rate)

  Timing Statistics:
    Mean: 757.8ms | Median: 753.2ms | Std Dev: 89.9ms
    P95: 892.0ms | P99: 906.8ms | CV: 0.12
```

Severity levels: **Critical** (<50%), **High** (50–80%), **Medium** (80–95%), **Low** (>95%).

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

### Monorepo / workspace

Scan a monorepo and run tests across all detected projects:

```bash
# Discover and test all projects
testx workspace

# List projects without running
testx workspace --list

# Only test Rust and Python projects
testx workspace --filter rust,python

# Include directories normally skipped (e.g., packages/)
testx workspace --include packages

# Run sequentially instead of in parallel
testx workspace --sequential
```

### Test history & analytics

Track test health over time:

```bash
# Quick overview
testx history

# Flaky test report
testx history flaky

# Slowest tests trend
testx history slow

# Health score dashboard (A–F grading)
testx history health

# Last 50 runs
testx history runs --last 50
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
testx -w                 # Watch mode — re-run on file changes
testx --retries 3        # Retry failed tests 3 times
testx --reporter github  # Activate GitHub Actions reporter
testx --no-custom-adapters  # Disable custom adapter loading
testx run --filter "auth*"  # Filter tests by name pattern
testx run --exclude "*slow" # Exclude tests matching pattern
testx run --fail-fast       # Stop on first failure
testx run --coverage        # Enable code coverage
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

Define custom test runners in `testx.toml`:

```toml
[[custom_adapter]]
name = "my-framework"
detect = "my-config.json"           # Simple: single file trigger
command = "my-test-runner"
args = ["--reporter", "json"]
output = "json"                     # json | junit | tap | lines
confidence = 0.8
check = "my-test-runner --version"   # Verify runner is installed
```

For advanced detection (multiple files, content matching, env vars):

```toml
[[custom_adapter]]
name = "make-test"
command = "make test"
output = "lines"
confidence = 0.85

[custom_adapter.detect]
files = ["Makefile", "test.mk"]
commands = ["make --version"]
env = ["CI"]
search_depth = 2

[[custom_adapter.detect.content]]
file = "Makefile"
contains = "test:"
```

### Global adapters

Place adapter definitions in `~/.config/testx/adapters/*.toml` to make them available across all projects:

```toml
# ~/.config/testx/adapters/bazel.toml
name = "bazel"
detect = "BUILD"
command = "bazel test //..."
output = "tap"
confidence = 0.7
```

### List adapters

```bash
testx adapters   # Show built-in, project, and global adapters
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
cargo test            # Run all tests (1150 tests)
cargo clippy          # Lint (0 warnings)
cargo fmt --check     # Format check
```

## Stats

| Metric                | Value                                                |
| --------------------- | ---------------------------------------------------- |
| Languages supported   | 11 built-in + custom adapters                        |
| Test frameworks       | 20+                                                  |
| Source lines          | ~36,000                                              |
| Test count            | 1,150 (1,096 unit + 33 CLI + 21 integration)         |
| Binary size (release) | ~2.2 MB                                              |
| Framework detection   | < 200 µs                                             |
| Rust source files     | 55                                                   |
| Dependencies          | minimal (clap, serde, colored, toml, anyhow, notify) |
| Clippy warnings       | 0                                                    |

## Performance

testx adds negligible overhead on top of your test runner:

| Operation                         | Time      |
| --------------------------------- | --------- |
| Config loading (no file)          | ~1.4 µs   |
| Config loading (with testx.toml)  | ~18 µs    |
| Framework detection (single lang) | ~67–83 µs |
| Framework detection (polyglot)    | ~149 µs   |
| Detect + parse 100 tests (Rust)   | ~138 µs   |
| Detect + parse 100 tests (Python) | ~161 µs   |
| Detect + parse 100 tests (Go)     | ~173 µs   |
| Parse 1,000 tests (Rust)          | ~570 µs   |
| Parse 5,000 tests (Rust)          | ~3 ms     |
| JSON serialization (1,000 tests)  | ~422 µs   |

**Total testx overhead: < 1 ms** for typical projects. Run `cargo bench --bench overhead` to reproduce.

## Documentation

Full documentation is available at **[testx-cli.readthedocs.io](https://testx-cli.readthedocs.io/)**.

- [Getting Started](https://testx-cli.readthedocs.io/getting-started/installation/)
- [Configuration Guide](https://testx-cli.readthedocs.io/guide/configuration/)
- [Output Formats](https://testx-cli.readthedocs.io/guide/output-formats/)
- [CI Sharding](https://testx-cli.readthedocs.io/guide/sharding/)
- [Plugin System](https://testx-cli.readthedocs.io/guide/plugins/)
- [CLI Reference](https://testx-cli.readthedocs.io/cli/)

## Contributing

We welcome contributions! Please read these before getting started:

- **[Contributing Guide](CONTRIBUTING.md)** — Setup, workflow, and PR process
- **[Coding Guidelines](.github/CODING_GUIDELINES.md)** — Architecture, code style, and module design
- **[Bug Report Template](https://github.com/whoisdinanath/testx/issues/new?template=bug_report.md)** — Report a bug
- **[Feature Request Template](https://github.com/whoisdinanath/testx/issues/new?template=feature_request.md)** — Suggest a feature

Pull requests are reviewed against the [PR template](.github/PULL_REQUEST_TEMPLATE.md) checklist.

## License

[MIT](LICENSE)
