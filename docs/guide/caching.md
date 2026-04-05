# Smart Caching

Smart caching lets testx **skip re-running tests when nothing has changed** since the last passing run. This can save significant time during local development when you're editing non-test files (docs, config, etc.) and don't need to re-run the full suite every time.

---

## How to use it

```bash
testx --cache
```

That's it. On a cache hit, testx prints the cached result instantly and skips execution entirely. If the previous run failed, tests are always re-run regardless of caching.

---

## How it works

When you run `testx --cache`, the following happens:

1. **Hash computation** — testx scans all source files in your project (matching the detected language) and computes a content hash based on file paths, modification times, sizes, and the adapter name
2. **Cache lookup** — It checks `.testx/cache.json` for a matching hash
3. **Hit or miss:**
    - **Cache hit** (hash matches + last run passed) → testx prints the cached result and exits immediately. No tests are executed.
    - **Cache miss** (hash doesn't match, or last run failed) → testx runs the tests normally and stores the new result

### What gets hashed

testx walks your project directory and includes files matching the detected language's file extensions (e.g., `.py` for Python, `.rs` for Rust, `.ts` for TypeScript).

These directories are always skipped during the walk:

- Hidden directories (`.git`, `.vscode`, etc.)
- `target`, `node_modules`, `__pycache__`
- `build`, `dist`, `vendor`
- `.testx`

!!! note "Different args = different cache keys"
    The cache key includes any extra arguments you pass. So `testx --cache` and `testx --cache -- --release` are treated as different runs and won't share a cache entry.

---

## Cache storage

- **Location:** `<project>/.testx/cache.json`
- **Max entries:** 100 (oldest entries are evicted)
- **Max age:** 24 hours — stale entries are pruned automatically

Each cache entry records the adapter name, pass/fail counts, duration, and any extra arguments used.

---

## Clearing the cache

To force a full re-run on the next invocation:

```bash
testx cache-clear
```

This removes all entries from `.testx/cache.json`.

---

## When to use caching

**Good use cases:**

- **Local development** — You're editing docs, README, or config files and don't need to re-run all tests each time
- **CI monorepos** — Combine with `--affected` to skip both unchanged shards and unchanged projects

**When NOT to use it:**

- When you want to catch flaky tests (caching hides them since a passing result is reused)
- When test behavior depends on external state (databases, APIs, time-of-day) that isn't captured by file hashes
