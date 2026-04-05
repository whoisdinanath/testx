# Monorepo / Workspace

The `testx workspace` command scans a directory tree, discovers all projects with test frameworks, and runs tests across all of them — in parallel by default.

## Basic usage

```bash
# Discover and test all projects
testx workspace

# List detected projects without running
testx workspace --list
```

Example output with `--list`:

```
Discovered 5 projects:
  1. apps/api         → Rust (cargo test)
  2. apps/web         → JavaScript (vitest)
  3. libs/auth        → Python (pytest)
  4. libs/utils       → Go (go test)
  5. tools/cli        → Rust (cargo test)
```

## Filtering by language

Run only specific languages:

```bash
# Only Rust and Python projects
testx workspace --filter rust,python

# Only JavaScript/TypeScript
testx workspace --filter javascript
```

## Including skipped directories

By default, testx skips common non-project directories like `node_modules`, `target`, `vendor`, `packages`, etc. Use `--include` to override:

```bash
# Include packages/ directory (common in monorepos)
testx workspace --include packages

# Include multiple directories
testx workspace --include packages,vendor
```

**Default skip list:** `.git`, `node_modules`, `target`, `build`, `dist`, `vendor`, `venv`, `__pycache__`, `.tox`, `.gradle`, `.idea`, `.vscode`, `bin`, `obj`, `packages`, `zig-cache`, `_build`, `deps`, `.bundle`, `.cargo`

## Controlling parallelism

```bash
# Auto-detect CPU count (default)
testx workspace --jobs 0

# Use 4 parallel workers
testx workspace --jobs 4

# Run one project at a time
testx workspace --sequential
```

## Fail-fast mode

Stop on the first project failure:

```bash
testx workspace --fail-fast
```

## Scan depth

Control how deep the directory scan goes:

```bash
# Scan up to 3 levels deep
testx workspace --max-depth 3

# Unlimited depth
testx workspace --max-depth 0
```

The default depth is 5 levels.

## Options reference

| Flag             | Short | Type    | Default | Description                                                         |
| ---------------- | ----- | ------- | ------- | ------------------------------------------------------------------- |
| `--max-depth`    |       | N       | `5`     | Maximum directory depth (0 = unlimited)                             |
| `--jobs`         | `-j`  | N       | `0`     | Parallel jobs (0 = auto-detect CPUs)                                |
| `--sequential`   |       | —       | —       | Run projects one at a time                                          |
| `--fail-fast`    |       | —       | —       | Stop on first project failure                                       |
| `--filter`       |       | STRING  | —       | Filter by language (comma-separated)                                |
| `--include`      |       | STRING  | —       | Include normally skipped directories                                |
| `--list`         |       | —       | —       | List projects without running                                       |
