# Impact Analysis

Impact analysis lets you **only run tests when relevant source files have changed**. If you're working on the Python backend and haven't touched any JavaScript files, there's no need to re-run the JavaScript tests. testx uses git to figure out what changed and decides whether tests need to run.

---

## Quick start

The simplest way to use impact analysis is with the `--affected` flag:

```bash
# Skip tests if no test-relevant files changed (compared to HEAD)
testx --affected

# Only consider staged changes
testx --affected=staged

# Compare against the main branch (great for CI on PRs)
testx --affected=branch:main
```

If nothing relevant changed, testx prints a message and exits with code 0 — no tests are executed.

---

## Standalone analysis (dry run)

If you just want to **see** what changed without running tests, use the `impact` command:

```bash
testx impact
testx impact --mode staged
testx impact --mode branch:main
testx impact --mode commit:abc1234
```

This shows:

- Total number of changed files
- How many are relevant vs irrelevant to test adapters
- Which adapters are affected
- Whether tests should run

This is useful for debugging or understanding why tests were skipped/run.

---

## Diff modes

testx supports four modes for determining what "changed" means:

| Mode             | What it compares                                         | Best for                                |
| ---------------- | -------------------------------------------------------- | --------------------------------------- |
| `head` (default) | Uncommitted changes + untracked files vs HEAD            | Local development                       |
| `staged`         | Only staged files (`git diff --cached`)                  | Pre-commit hooks                        |
| `branch:<name>`  | Changes since the merge-base with the given branch        | CI on pull requests (compare to `main`) |
| `commit:<sha>`   | Changes since a specific commit                          | Custom CI workflows                     |

**Example — PR pipeline:**

```bash
# In CI, only run tests affected by changes in this PR
testx --affected=branch:main
```

**Example — pre-commit hook:**

```bash
# Only run tests if staged files are test-relevant
testx --affected=staged
```

---

## How testx determines relevance

testx maps file extensions to language adapters. When a file changes, testx checks whether its extension belongs to any adapter:

| Language   | Relevant extensions                                                   |
| ---------- | --------------------------------------------------------------------- |
| Rust       | `.rs`, `.toml`                                                        |
| Go         | `.go`, `.mod`, `.sum`                                                 |
| Python     | `.py`, `.pyi`, `.cfg`, `.ini`, `.toml`                                |
| JavaScript | `.js`, `.jsx`, `.ts`, `.tsx`, `.mjs`, `.cjs`, `.json`                 |
| Java       | `.java`, `.kt`, `.kts`, `.gradle`, `.xml`, `.properties`              |
| C/C++      | `.cpp`, `.cc`, `.cxx`, `.c`, `.h`, `.hpp`, `.cmake`                   |
| Ruby       | `.rb`, `.rake`, `.gemspec`                                            |
| Elixir     | `.ex`, `.exs`                                                         |
| PHP        | `.php`, `.xml`                                                        |
| .NET       | `.cs`, `.fs`, `.vb`, `.csproj`, `.fsproj`, `.sln`                     |
| Zig        | `.zig`                                                                |

**Special cases:**

- Config files like `Cargo.toml`, `package.json`, `go.mod`, and `pyproject.toml` are considered relevant to **all** adapters (changes to build config could affect any test)
- The `.testx/` directory is always excluded from relevance checks

---

## Combining with other features

Impact analysis works well with sharding and caching:

```bash
# Skip the shard entirely if nothing in it changed
testx --affected=branch:main --partition slice:1/4

# Cache + impact — skip if nothing changed OR results are cached
testx --affected --cache
```

This is particularly powerful in CI — you can skip entire jobs when a PR only touches files unrelated to that job's tests.
