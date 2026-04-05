# Configuration

testx works out of the box with **zero configuration** — it auto-detects your language and test framework. But when you need to customize behavior (set timeouts, add environment variables, override the detected framework, etc.), you can create a `testx.toml` config file.

---

## Getting started with configuration

### Generate a config file

The easiest way to start is to let testx generate one for you:

```bash
testx init
```

This creates a `testx.toml` in the current directory with your detected adapter and common options pre-filled (most commented out). You can then uncomment and edit whatever you need.

### Where does testx look for config?

testx looks for `testx.toml` in the project root (the directory you run `testx` from). If it doesn't find one, it uses sensible defaults.

---

## Config file reference

Here's a complete `testx.toml` with every option explained:

```toml
# ─── Adapter selection ───────────────────────────────────────────
# Which test framework to use. Default: "auto" (testx detects it).
# Set this to force a specific framework, e.g. "pytest", "jest", "cargo".
adapter = "auto"

# ─── Test runner arguments ───────────────────────────────────────
# Extra arguments passed directly to the test runner.
# These go AFTER the base command. Same as using "testx -- <args>"
args = ["--release", "--", "--nocapture"]

# ─── Timeout ─────────────────────────────────────────────────────
# Kill the test process if it runs longer than N seconds.
# 0 = no timeout. Useful in CI to prevent hanging tests.
timeout = 60

# ─── Fail fast ───────────────────────────────────────────────────
# Stop running tests as soon as the first failure occurs.
# Speeds up feedback when you just want to know if something broke.
fail_fast = true

# ─── Retries ─────────────────────────────────────────────────────
# If a test fails, retry it up to N times. A test passes if any
# retry succeeds. Useful for flaky tests.
retries = 3

# ─── Parallel ────────────────────────────────────────────────────
# When multiple adapters are detected (e.g., both Python and JS tests),
# run them in parallel instead of sequentially.
parallel = true

# ─── Environment variables ───────────────────────────────────────
# Set environment variables for the test runner process.
[env]
CI = "true"
DATABASE_URL = "sqlite::memory:"

# ─── Test filtering ──────────────────────────────────────────────
# Only run tests matching certain patterns.
[filter]
include = "test_*"         # Only run tests whose names match this pattern
exclude = "*_slow"         # Skip tests whose names match this pattern

# ─── Watch mode ──────────────────────────────────────────────────
# Settings for "testx -w" (automatic re-run on file changes).
[watch]
enabled = false            # Set to true to always use watch mode
clear = true               # Clear the terminal before each re-run
debounce_ms = 300          # Wait this long after a change before re-running
poll_ms = 0                # 0 = use native FS events (faster)
                           # Set >0 for NFS/network drives/containers
ignore = [                 # Files/directories to ignore for change detection
  "*.pyc",
  "__pycache__",
  ".git",
  "node_modules",
  "target",
  ".testx"
]

# ─── Output settings ─────────────────────────────────────────────
[output]
format = "pretty"          # pretty | json | junit | tap
slowest = 5                # Show the N slowest tests at the end
verbose = false            # Show extra details (detected command, etc.)
colors = "auto"            # auto | always | never

# ─── Coverage ────────────────────────────────────────────────────
# Collect code coverage data during the test run.
[coverage]
enabled = false
format = "summary"         # summary | lcov | html | cobertura
output_dir = "coverage"    # Directory where coverage reports are written
threshold = 80.0           # Fail the run if coverage is below this %

# ─── History / analytics ─────────────────────────────────────────
# testx tracks test results over time for trend analysis.
[history]
enabled = true
max_age_days = 30          # Prune history entries older than this
db_path = ".testx/history.db"
```

---

## Per-adapter overrides

You can override settings for a specific language or framework. This is useful when you have a monorepo with different needs per language:

```toml
# General settings
timeout = 60

# Python tests get a different timeout and extra args
[adapters.python]
runner = "pytest"
args = ["-x", "--tb=short"]
timeout = 120

[adapters.python.env]
PYTHONPATH = "src"
```

The per-adapter settings are merged on top of your general settings. So in this example, Python tests use timeout=120, while everything else uses timeout=60.

---

## Custom adapters

If testx doesn't natively support your test framework, you can define a **custom adapter**. This tells testx how to detect and run your framework.

### Basic custom adapter

```toml
[[custom_adapter]]
name = "my-framework"
detect = "myframework.config"    # If this file exists, use this adapter
command = "myfw test"            # The command to run tests
args = ["--verbose"]             # Extra arguments
output = "lines"                 # How to parse output: json | junit | tap | lines
confidence = 0.5                 # Detection confidence (0.0 – 1.0)
check = "myfw --version"         # Verify the runner is installed before running
working_dir = "tests"            # Run from this directory (relative to project root)

[custom_adapter.env]
MY_VAR = "value"
```

**Fields explained:**

- `name` — A label for this adapter (shown in `testx detect` output)
- `detect` — A file whose existence triggers this adapter
- `command` — The shell command to run tests
- `output` — How testx parses the output (`lines` = no parsing, just pass through)
- `confidence` — How confident testx should be when this adapter matches (higher = preferred over other matches)
- `check` — A command that must succeed (exit code 0) for the adapter to be usable

### Advanced detection (multiple signals)

For more precise detection, use a `[detect]` table with multiple conditions:

```toml
[[custom_adapter]]
name = "make-test"
command = "make test"
output = "lines"
confidence = 0.85

[custom_adapter.detect]
files = ["Makefile", "test.mk"]         # At least one of these must exist
commands = ["make --version"]            # All must succeed (exit 0)
env = ["CI"]                             # All env vars must be set
search_depth = 2                         # How deep to search for files

[[custom_adapter.detect.content]]
file = "Makefile"
contains = "test:"                       # The file must contain this string
```

This adapter only activates when:

1. A `Makefile` or `test.mk` exists (within 2 directories deep), AND
2. `make --version` succeeds, AND
3. The `CI` environment variable is set, AND
4. The `Makefile` contains the string `test:`

### Global custom adapters

If you want an adapter available in **all** your projects (not just one), place `.toml` files in:

```
~/.config/testx/adapters/
```

(Or `$XDG_CONFIG_HOME/testx/adapters/` on Linux.)

Each file can contain one or more `[[custom_adapter]]` blocks.

### Managing adapters

```bash
# List all adapters (built-in + project + global custom)
testx adapters

# Run tests but ignore all custom adapters
testx --no-custom-adapters
```

---

## Precedence: CLI vs config file

CLI flags always override `testx.toml` values. For example:

```toml
# testx.toml
[output]
format = "pretty"
```

```bash
# This uses JSON output, overriding the config file
testx -o json
```

The full precedence order (highest to lowest):

1. CLI flags (`testx --timeout 30`)
2. `testx.toml` in the project root
3. Built-in defaults

---

## Environment variables

These environment variables affect testx regardless of config:

| Variable    | Effect                  |
| ----------- | ----------------------- |
| `NO_COLOR`  | Disables colored output |
| `TERM=dumb` | Disables colored output |
