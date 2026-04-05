# testx

**One command. Any language. Beautiful tests.**

testx is a universal test runner that auto-detects your project's language and test framework, runs your tests, and shows clean, unified output. No configuration needed.

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

## Why testx?

- **Zero config** — just run `testx` in any project
- **11 languages** — Rust, Go, Python, JS/TS, Java, C/C++, Ruby, Elixir, PHP, .NET, Zig
- **Monorepo support** — scan and test all projects with `testx workspace`
- **CI-ready** — sharding, caching, impact analysis, JUnit/JSON/TAP output
- **Flaky test detection** — stress test mode runs N times and reports pass rates
- **Test analytics** — history tracking with health scores, flaky detection, slowest tests
- **Fast** — smart caching skips re-runs when nothing changed

## Quick example

```bash
# Run tests (auto-detects framework)
testx

# Only test what changed
testx --affected

# Split across 4 CI nodes
testx --partition slice:1/4

# Find flaky tests
testx stress -n 20

# Fuzzy-pick tests to run
testx pick

# Test all projects in a monorepo
testx workspace

# View test health dashboard
testx history health
```

## Getting started

See the [installation guide](getting-started/installation.md) to get started.
