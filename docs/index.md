# testx

**One command. Any language. Beautiful tests.**

testx is a **universal test runner** — a single command that works across all your projects, regardless of language or framework. It auto-detects what you're using, runs your tests, and shows clean, consistent output. No configuration needed.

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

## Why testx?

If you work across multiple projects or languages, you know the pain: each one has a different test command (`cargo test`, `pytest`, `npm test`, `go test`, `dotnet test`, ...). testx replaces all of them with one command: `testx`.

| Feature | What it does |
| ------- | ------------ |
| **Zero config** | Auto-detects language + framework. Just run `testx`. |
| **11 languages** | Rust, Go, Python, JavaScript/TypeScript, Java, C/C++, Ruby, Elixir, PHP, .NET, Zig |
| **Monorepo support** | Discover and test all projects with `testx workspace` |
| **CI-ready** | Sharding, caching, impact analysis, JUnit/JSON/TAP output |
| **Flaky test detection** | Stress-test mode runs N times and classifies flaky tests by severity |
| **Test analytics** | History tracking with health scores, trends, and slowest tests |
| **Smart caching** | Skip re-runs when nothing changed |
| **Custom adapters** | Add support for any framework via `testx.toml` |

---

## Quick examples

```bash
# Run tests — auto-detects your framework
testx

# See what testx detected (without running tests)
testx detect

# Run with JSON output for CI
testx -o json

# Pass extra flags to the underlying test runner
testx -- -k "test_login"

# Watch mode — re-run on file changes
testx -w

# Only test what changed since the main branch
testx --affected=branch:main

# Split tests across 4 CI nodes
testx --partition slice:1/4

# Find flaky tests by running 20 times
testx stress -n 20

# Fuzzy-pick tests to run interactively
testx pick

# Test all projects in a monorepo
testx workspace

# View test health dashboard
testx history health
```

---

## Getting started

1. **[Install testx](getting-started/installation.md)** — via npm, install script, cargo, or binary download
2. **[Quick Start](getting-started/quickstart.md)** — run your first tests and learn the basics
3. **[Supported Languages](languages/index.md)** — see all 11 supported languages and how detection works

## Guides

| Guide | Description |
| ----- | ----------- |
| [Configuration](guide/configuration.md) | Customize testx with `testx.toml` |
| [Output Formats](guide/output-formats.md) | JSON, JUnit XML, TAP output for CI |
| [CI Sharding](guide/sharding.md) | Split tests across CI nodes |
| [Smart Caching](guide/caching.md) | Skip tests when nothing changed |
| [Impact Analysis](guide/impact-analysis.md) | Only run tests affected by code changes |
| [Flaky Test Detection](guide/stress-testing.md) | Find intermittent failures |
| [Test History](guide/history.md) | Analytics, trends, health scores |
| [Monorepo/Workspace](guide/workspace.md) | Test across multiple projects |
| [Interactive Picker](guide/picker.md) | Fuzzy-search and select tests |
| [Plugins](guide/plugins.md) | Custom reporters and adapters |

## Reference

- [CLI Reference](cli.md) — all commands, flags, and options
- [Changelog](changelog.md) — release history
