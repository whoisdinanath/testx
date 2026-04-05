# Monorepo / Workspace

If you have a **monorepo** (a single repository containing multiple projects), the `testx workspace` command can discover all the projects inside it and run their tests — in parallel by default.

This means you don't need to `cd` into each project and run tests separately. One command tests everything.

---

## Basic usage

```bash
# Discover and test all projects in the current directory tree
testx workspace
```

testx will recursively scan your directory, find all projects with recognized test frameworks, and run their tests.

### Dry run — just list what's detected

```bash
testx workspace --list
```

Example output:

```
Discovered 5 projects:
  1. apps/api         → Rust (cargo test)
  2. apps/web         → JavaScript (vitest)
  3. libs/auth        → Python (pytest)
  4. libs/utils       → Go (go test)
  5. tools/cli        → Rust (cargo test)
```

This is useful for verifying that testx found all your projects before actually running anything.

---

## Filtering by language

If you only want to test certain languages:

```bash
# Only Rust and Python projects
testx workspace --filter rust,python

# Only JavaScript/TypeScript
testx workspace --filter javascript
```

---

## Including normally-skipped directories

By default, testx skips common non-project directories to avoid false detections and speed up scanning. The full skip list:

`.git`, `node_modules`, `target`, `build`, `dist`, `vendor`, `venv`, `__pycache__`, `.tox`, `.gradle`, `.idea`, `.vscode`, `bin`, `obj`, `packages`, `zig-cache`, `_build`, `deps`, `.bundle`, `.cargo`

If your projects live inside one of these directories (common in monorepo setups with a `packages/` folder), use `--include`:

```bash
# Scan inside the packages/ directory
testx workspace --include packages

# Include multiple directories
testx workspace --include packages,vendor
```

---

## Controlling parallelism

By default, testx runs all discovered projects **in parallel** using as many CPU cores as available. You can control this:

```bash
# Auto-detect CPU count (default)
testx workspace --jobs 0

# Limit to 4 parallel workers
testx workspace --jobs 4

# Run one project at a time (useful for debugging or resource-constrained environments)
testx workspace --sequential
```

---

## Fail-fast mode

Stop testing as soon as any project fails:

```bash
testx workspace --fail-fast
```

Without this, testx runs all projects and reports all failures at the end.

---

## Scan depth

Control how deep testx looks for projects:

```bash
# Only scan 3 levels deep
testx workspace --max-depth 3

# Unlimited depth (can be slow in very large trees)
testx workspace --max-depth 0
```

The default is 5 levels, which works well for most monorepo structures.

---

## Options reference

| Flag           | Short | Type   | Default | Description                                            |
| -------------- | ----- | ------ | ------- | ------------------------------------------------------ |
| `--max-depth`  |       | N      | `5`     | Maximum directory depth to scan                        |
| `--jobs`       | `-j`  | N      | `0`     | Parallel jobs (0 = auto-detect CPU count)              |
| `--sequential` |       | —      | —       | Run projects one at a time                             |
| `--fail-fast`  |       | —      | —       | Stop on first project failure                          |
| `--filter`     |       | STRING | —       | Filter by language (comma-separated)                   |
| `--include`    |       | STRING | —       | Include directories that are normally skipped           |
| `--list`       |       | —      | —       | List discovered projects without running tests         |

---

## Tips

- Use `testx workspace --list` first to check detection before running tests
- In CI, combine with sharding for faster pipelines: `testx workspace --filter rust --partition slice:1/2`
- Use `--sequential` when debugging to see output from each project in order
- Add a `testx.toml` in individual project directories to customize their behavior (timeouts, args, etc.)
