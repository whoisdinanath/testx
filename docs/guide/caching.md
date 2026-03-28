# Smart Caching

Skip re-running tests when nothing has changed since the last passing run.

## Usage

```bash
testx --cache
```

On a cache hit, testx prints the cached result and skips execution entirely. If the previous run failed, tests are always re-run.

## How it works

1. testx computes a **content hash** from all source files (using file path, modification time, size, and adapter name)
2. Looks up the hash in `.testx/cache.json`
3. If a matching entry exists **and the previous run passed** — cache hit, skip execution
4. Otherwise, runs tests and stores the result

## Cache storage

- **Location**: `<project>/.testx/cache.json`
- **Max entries**: 100
- **Max age**: 24 hours — stale entries are pruned automatically

Each cache entry records: adapter name, pass/fail counts, duration, and any extra args used.

!!! note
    Different adapters and different extra args produce different cache keys, so `testx --cache -- --release` won't match a cache entry from `testx --cache`.

## Clearing the cache

```bash
testx cache-clear
```

This removes all entries from `.testx/cache.json`.

## What gets hashed

testx walks the project directory recursively and includes files matching the detected language's extensions. The following directories are always skipped:

- Hidden directories (`.git`, `.vscode`, etc.)
- `target`, `node_modules`, `__pycache__`
- `build`, `dist`, `vendor`
- `.testx`

## When to use

- **Local development** — avoid re-running a passing suite while editing docs or config
- **CI** — combine with `--affected` to skip both unchanged shards and unchanged projects in a monorepo
