# Configuration

testx works with zero configuration. For customization, create a `testx.toml` in your project root.

## Generate a config file

```bash
testx init
```

This creates a `testx.toml` with the detected adapter and common options commented out.

## Full reference

```toml
# Override adapter selection (default: auto-detect)
adapter = "auto"

# Extra arguments passed to the test runner
args = ["--release", "--", "--nocapture"]

# Kill test process after N seconds (0 = no timeout)
timeout = 60

# Stop on first failure
fail_fast = true

# Retries for failed tests
retries = 3

# Run all detected adapters in parallel
parallel = true

# Environment variables
[env]
CI = "true"
DATABASE_URL = "sqlite::memory:"

# Test name filtering
[filter]
include = "test_*"
exclude = "*_slow"

# Watch mode
[watch]
enabled = false
clear = true
debounce_ms = 300
poll_ms = 0                # 0 = native FS events; set >0 for NFS/network drives
ignore = ["*.pyc", "__pycache__", ".git", "node_modules", "target", ".testx"]

# Output settings
[output]
format = "pretty"          # pretty | json | junit | tap
slowest = 5                # Show N slowest tests
verbose = false
colors = "auto"            # auto | always | never

# Coverage
[coverage]
enabled = false
format = "summary"         # summary | lcov | html | cobertura
output_dir = "coverage"
threshold = 80.0           # Fail if coverage is below this %

# History / analytics
[history]
enabled = false
max_age_days = 30
db_path = ".testx/history.db"
```

## Per-adapter overrides

Override settings for a specific adapter:

```toml
[adapters.python]
runner = "pytest"
args = ["-x", "--tb=short"]
timeout = 120

[adapters.python.env]
PYTHONPATH = "src"
```

## Custom adapters

Define adapters for frameworks testx doesn't natively support:

```toml
[[custom_adapter]]
name = "my-framework"
detect = "myframework.config"    # File that triggers detection
command = "myfw test"
args = ["--verbose"]
parse = "lines"                  # json | junit | tap | lines | regex
confidence = 0.5
```

## Precedence

CLI flags override `testx.toml` values. For example:

```bash
# Uses JSON output even if testx.toml says "pretty"
testx -o json
```

## Environment variables

| Variable    | Effect                  |
| ----------- | ----------------------- |
| `NO_COLOR`  | Disables colored output |
| `CI`        | Disables colored output |
| `TERM=dumb` | Disables colored output |
