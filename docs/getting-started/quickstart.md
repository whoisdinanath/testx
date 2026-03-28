# Quick Start

## Run tests

Navigate to any project directory and run:

```bash
testx
```

testx will auto-detect the language and test framework, then run your tests with formatted output.

## Detect without running

```bash
testx detect
```

This shows what testx detected without actually running anything:

```
Detected: Python (pytest) — confidence 0.95
```

## Pass arguments to the test runner

Use `--` to pass extra arguments through:

```bash
testx -- --filter my_test
testx -- -k "test_login"        # pytest filter
testx -- --test-threads=1       # cargo test flag
```

## Run in a different directory

```bash
testx -p /path/to/project
```

## Generate a config file

```bash
testx init
```

Creates a `testx.toml` with defaults you can customize.

## Common workflows

```bash
# Show 5 slowest tests
testx --slowest 5

# Set a timeout (kills after 60 seconds)
testx --timeout 60

# Show raw test runner output
testx --raw

# Verbose mode (shows the detected command)
testx -v
```

## Next steps

- [Output formats](../guide/output-formats.md) — JSON, JUnit XML, TAP
- [CI sharding](../guide/sharding.md) — split tests across CI nodes
- [Flaky test detection](../guide/stress-testing.md) — find intermittent failures
- [Configuration](../guide/configuration.md) — customize with `testx.toml`
