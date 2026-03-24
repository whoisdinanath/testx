# testx

Universal test runner — one command to test any project.

**testx** auto-detects your project's language and test framework, runs your tests, and displays beautiful, unified output. No configuration needed.

## Supported Frameworks

| Language                  | Frameworks                   | Package Managers      |
| ------------------------- | ---------------------------- | --------------------- |
| **Python**                | pytest, unittest, Django     | uv, poetry, pdm      |
| **JavaScript/TypeScript** | vitest, jest, mocha, bun     | npm, pnpm, yarn, bun  |
| **Go**                    | go test                      | —                     |
| **Rust**                  | cargo test                   | —                     |
| **Java/Kotlin**           | Maven Surefire, Gradle       | mvn, gradle, gradlew  |
| **C/C++**                 | CTest, Meson                 | cmake, meson          |
| **Ruby**                  | RSpec, Minitest              | bundler               |
| **Elixir**                | ExUnit                       | mix                   |
| **PHP**                   | PHPUnit                      | composer              |
| **C#/.NET / F#**          | dotnet test                  | dotnet                |
| **Zig**                   | zig test                     | zig                   |

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Run tests (auto-detect framework)
testx

# Detect framework without running
testx detect

# List supported frameworks
testx list

# Generate a testx.toml config file
testx init

# Pass extra args to the underlying runner
testx -- --filter my_test

# Output formats: pretty (default), json, junit, tap
testx -o json
testx -o junit > report.xml
testx -o tap

# Show 5 slowest tests
testx --slowest 5

# Set a timeout (in seconds)
testx --timeout 60

# Run in a different directory
testx -p /path/to/project

# Verbose mode (show detected command)
testx -v

# Show raw test runner output
testx --raw
```

## Configuration

Create a `testx.toml` in your project root (or use `testx init`):

```toml
# Override adapter selection (auto-detected by default)
# adapter = "python"

# Extra arguments to pass to the test runner
args = ["-v", "--no-header"]

# Timeout in seconds (0 = no timeout)
timeout = 60

# Environment variables
[env]
CI = "true"
RUST_LOG = "debug"
```

CLI flags always take precedence over config file values.

## Example Output

```
testx · Python (pytest)
────────────────────────────────────────────────────────────

  tests/test_math.py
    ✓ test_add
    ✓ test_subtract
    ✗ test_divide

────────────────────────────────────────────────────────────
  FAIL 2 passed, 1 failed (3 total) in 120ms
```

## Output Formats

**JUnit XML** — for CI integration (Jenkins, GitLab CI, etc.):
```bash
testx -o junit > test-results.xml
```

**TAP** — Test Anything Protocol:
```bash
testx -o tap
```

**JSON** — machine-readable structured output:
```bash
testx -o json
```

## License

MIT
