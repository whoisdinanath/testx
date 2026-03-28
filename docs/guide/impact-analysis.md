# Impact Analysis

Only run tests when relevant files have changed.

## Quick start

```bash
# Skip tests if nothing test-relevant changed
testx --affected

# Only check staged changes
testx --affected=staged

# Compare against main branch
testx --affected=branch:main
```

## Standalone analysis

Use `testx impact` to see what changed without running tests:

```bash
testx impact
testx impact --mode staged
testx impact --mode branch:main
testx impact --mode commit:abc1234
```

Output includes:

- Total changed files
- Relevant vs irrelevant files
- Which adapters are affected
- Whether tests should run

## Diff modes

| Mode | Description |
|------|-------------|
| `head` (default) | Uncommitted changes + untracked files vs HEAD |
| `staged` | Only staged files (`git diff --cached`) |
| `branch:<name>` | Changes since merge-base with the given branch |
| `commit:<sha>` | Changes since a specific commit |

## How it works

testx maps file extensions to language adapters:

| Language | Extensions |
|----------|-----------|
| Rust | `.rs`, `.toml` |
| Go | `.go`, `.mod`, `.sum` |
| Python | `.py`, `.pyi`, `.cfg`, `.ini`, `.toml` |
| JavaScript | `.js`, `.jsx`, `.ts`, `.tsx`, `.mjs`, `.cjs`, `.json` |
| Java | `.java`, `.kt`, `.kts`, `.gradle`, `.xml`, `.properties` |
| C/C++ | `.cpp`, `.cc`, `.cxx`, `.c`, `.h`, `.hpp`, `.cmake` |
| Ruby | `.rb`, `.rake`, `.gemspec` |
| Elixir | `.ex`, `.exs` |
| PHP | `.php`, `.xml` |
| .NET | `.cs`, `.fs`, `.vb`, `.csproj`, `.fsproj`, `.sln` |
| Zig | `.zig` |

Config files like `Cargo.toml`, `package.json`, `go.mod`, and `pyproject.toml` are considered relevant to all adapters.

The `.testx/` directory is always excluded from relevance checks.

## CI usage

Combine with sharding to skip entire shards when nothing changed:

```bash
testx --affected=branch:main --partition slice:1/4
```
