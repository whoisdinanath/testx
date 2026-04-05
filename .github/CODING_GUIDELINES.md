# Coding Guidelines

This document defines the coding standards, architecture conventions, and modular design principles for the testx codebase. All contributors must follow these guidelines.

## Architecture Overview

testx follows a **modular, trait-driven architecture**. Each major subsystem is isolated into its own module with well-defined boundaries.

```
src/
├── main.rs              # CLI entry point (clap parsing, command dispatch)
├── lib.rs               # Public module re-exports
├── adapters/            # Language-specific test adapters (one file per language)
│   ├── mod.rs           # TestAdapter trait + DetectionEngine
│   ├── rust.rs          # Rust/Cargo adapter
│   ├── python.rs        # Python adapter (pytest, unittest)
│   ├── javascript.rs    # JavaScript adapter (jest, vitest, mocha)
│   ├── go.rs            # Go adapter
│   ├── java.rs          # Java/Kotlin adapter (gradle, maven)
│   ├── cpp.rs           # C/C++ adapter (ctest, gtest)
│   ├── ruby.rs          # Ruby adapter (rspec, minitest)
│   ├── elixir.rs        # Elixir adapter (exunit)
│   ├── php.rs           # PHP adapter (phpunit)
│   ├── dotnet.rs        # .NET adapter
│   ├── zig.rs           # Zig adapter
│   └── util.rs          # Shared parsing utilities
├── detection/           # Framework detection engine
├── output/              # Output formatters (pretty, JSON, JUnit, TAP)
├── plugin/              # Plugin system (reporters, script adapters)
│   └── reporters/       # Built-in reporter plugins
├── coverage/            # Coverage tool integration
├── watcher/             # File watcher (native events via notify)
│   ├── file_watcher.rs  # Filesystem watcher backend
│   ├── debouncer.rs     # Event debouncing
│   ├── glob.rs          # Glob pattern matching
│   ├── runner.rs        # Watch-mode runner
│   └── terminal.rs      # Terminal handling for watch mode
├── config.rs            # Configuration (testx.toml parsing)
├── cache.rs             # Test result caching
├── events.rs            # Event bus for lifecycle hooks
├── filter.rs            # Test filtering (include/exclude patterns)
├── history/             # Test history and health tracking
├── impact.rs            # Git-based impact analysis
├── parallel.rs          # Parallel suite execution
├── runner.rs            # Core test runner
├── retry.rs             # Retry logic for flaky tests
├── sharding.rs          # CI sharding (slice/hash)
├── stress.rs            # Stress testing (repeated runs)
├── picker.rs            # Interactive test picker
├── completions.rs       # Shell completion generation
└── error.rs             # Error types
```

## Module Design Principles

### 1. One Responsibility Per Module

Each module owns a single, well-defined responsibility. Modules should not reach into the internals of other modules.

```rust
// GOOD: filter.rs owns all filtering logic
pub struct TestFilter { ... }
impl TestFilter {
    pub fn apply(&self, result: &TestRunResult) -> TestRunResult { ... }
}

// BAD: filtering logic scattered across main.rs and runner.rs
```

### 2. Trait-Based Abstraction

Use traits to define module boundaries. Consumers depend on traits, not concrete types.

```rust
// adapters/mod.rs — the central trait
pub trait TestAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn detect(&self, dir: &Path) -> Option<DetectionResult>;
    fn build_command(&self, dir: &Path, args: &[String]) -> Result<Command>;
    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult;
    fn check_runner(&self) -> Option<String>;
}
```

### 3. Adding a New Adapter

Each language adapter is a single file in `src/adapters/`. To add a new one:

1. Create `src/adapters/language.rs`
2. Implement `TestAdapter`
3. Register it in `DetectionEngine::new()` inside `src/detection/mod.rs`
4. Add comprehensive tests (detection, command building, output parsing)

An adapter must never depend on another adapter. Shared utilities go in `adapters/util.rs`.

### 4. Data Flows Down, Events Flow Up

- **Data** (config, CLI args) flows from `main.rs` into modules via function parameters.
- **Events** (RunStarted, RunFinished) flow from modules up through the `EventBus`.
- Modules must not read global state or access CLI args directly.

### 5. Config Merging Convention

CLI flags always take precedence over `testx.toml` config. The merge happens once in `main.rs`, then resolved values are passed into modules. Modules never read the config file themselves.

```rust
// GOOD: merge in main.rs, pass resolved value
let verbose = cli.verbose || config.output_config().verbose.unwrap_or(false);

// BAD: module reads config internally
fn run_tests() {
    let config = Config::load(&dir);  // Don't do this in a module
}
```

## Project Architecture Rationale

### Why Inline Tests (`#[cfg(test)] mod tests`)?

Rust has a **two-tier test architecture** baked into the language:

| Tier                  | Location                                     | Scope                                 | Build impact                                             |
| --------------------- | -------------------------------------------- | ------------------------------------- | -------------------------------------------------------- |
| **Unit tests**        | `#[cfg(test)] mod tests` inside source files | Can access private/`pub(crate)` items | Dead-code-eliminated from release builds — zero overhead |
| **Integration tests** | `tests/*.rs` at crate root                   | Only access `pub` API                 | Separate compilation unit                                |

This is **the canonical Rust convention**, used by the standard library, ripgrep, serde, tokio, clap, and essentially every major Rust project. It's documented in _The Rust Programming Language_ (Chapter 11) as the recommended approach.

Key reasons:

- **Tests live next to the code they test** — no context switching between files
- **Private function testing** — only possible from inline test modules
- **Zero cost** — `#[cfg(test)]` is compile-time conditional; test code never enters release binaries
- **Refactoring safety** — moving/renaming a function automatically moves its tests

### Why `lib.rs` + `main.rs`?

- `main.rs` is the **thin CLI entry point** — parses args, dispatches commands, handles exit codes
- `lib.rs` exports all business logic so it's testable from `tests/` without spawning a process
- Integration tests (`tests/cli.rs`) can only `use testx::*` — they can't access `main.rs` internals

### Why External Dependencies Like `notify`?

We use `notify` for file watching because:

- It's the **de facto standard** — used by cargo-watch, watchexec, mdBook, zola, bacon
- On Linux: wraps `inotify` (kernel-level, essentially zero CPU/memory overhead)
- On macOS: wraps FSEvents (single handle for recursive directory watching)
- Binary size impact: ~20-40 KB (negligible next to clap, serde)
- Rolling your own file watcher means re-discovering dozens of edge cases (editor atomic saves, rename vs delete+create, symlinks, NFS)

**Policy**: External dependencies are acceptable when they wrap OS-level APIs, are widely adopted (>1M downloads), and would take >500 lines to reimplement correctly. Avoid dependencies for things achievable in <50 lines.

## Rust Coding Standards

### Formatting & Linting

All code must pass these checks with zero warnings:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

### Naming

| Item            | Convention       | Example                          |
| --------------- | ---------------- | -------------------------------- |
| Types / Traits  | `PascalCase`     | `TestAdapter`, `DetectionResult` |
| Functions       | `snake_case`     | `parse_output`, `build_command`  |
| Constants       | `SCREAMING_CASE` | `MAX_COLLECT_DEPTH`              |
| Modules / Files | `snake_case`     | `file_watcher.rs`                |
| Feature flags   | `kebab-case`     | `macos_kqueue`                   |

### Error Handling

- Use `anyhow::Result` for application-level errors (main.rs, CLI commands).
- Use `std::io::Result` or custom error types for library code that other modules consume.
- Use `.context()` to add human-readable messages to errors.
- Never use `.unwrap()` in library code. Tests may use `.unwrap()`.

```rust
// GOOD
let config = Config::load(&dir).context("Failed to load testx.toml")?;

// BAD
let config = Config::load(&dir).unwrap();
```

### Function Design

- Keep functions **under 50 lines**. Extract helpers when a function grows beyond this.
- Functions should do one thing. If you're naming a function `parse_and_format_and_write`, split it.
- Prefer returning values over mutating arguments.
- Use `&str` over `&String`, `&Path` over `&PathBuf` in function signatures.

### Struct Design

- Prefer builder patterns or `::new()` constructors over public field initialization.
- Use `#[derive(Debug, Clone)]` on data types by default.
- Keep structs small — if a struct has more than 6-7 fields, consider grouping related fields.

### Testing

- Every adapter must have tests for: detection, command building, output parsing (pass/fail/skip/error).
- Unit tests go in `#[cfg(test)] mod tests` at the bottom of the file.
- Integration tests go in `tests/`.
- Use `tempfile::tempdir()` for tests that need filesystem state.
- Test edge cases: empty output, malformed input, missing binaries, Unicode.
- Safety tests: verify recursion depth limits, symlink loop protection, memory bounds.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_output() {
        let result = adapter.parse_output("", "", 0);
        assert_eq!(result.total_tests(), 0);
    }
}
```

### Dependencies

- Keep dependencies minimal. Justify any new dependency.
- Prefer `default-features = false` and opt in to specific features.
- Pin major versions only (`"1"` not `"1.0.102"` — except for stability-critical deps).

### Safety & Robustness

- All recursive functions must have a **depth limit** constant (e.g., `MAX_COLLECT_DEPTH`).
- Filesystem traversal must have **symlink loop protection** (canonicalize + visited set).
- No unbounded `Vec` growth in hot loops — use size caps or consume-on-report patterns.
- Always handle process exit codes — never assume a command succeeded.

### Documentation

- Public items (`pub fn`, `pub struct`, `pub trait`) should have a doc comment (`///`).
- Internal helpers don't require doc comments, but add one if the logic is non-obvious.
- Don't add comments that restate the code. Comment _why_, not _what_.

```rust
// GOOD: explains the why
/// Reduce confidence for workspace-only roots that lack a src/ directory,
/// since they're usually meta-crates that delegate to member packages.

// BAD: restates the code
/// Returns the name of the adapter.
fn name(&self) -> &str { "rust" }
```

## Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>(<scope>): <subject>

<body>
```

**Types:** `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `ci`, `chore`

**Examples:**

```
feat(adapter): add Zig language adapter
fix(rust): handle workspace-only Cargo.toml detection
test(plugin): add edge case tests for GitHub reporter
docs(readme): update stats and add coverage section
refactor(watcher): replace polling with notify crate
```

## File Organization Checklist

When adding a new module or feature:

- [ ] Module has a single, clear responsibility
- [ ] Public API is minimal — only expose what consumers need
- [ ] Trait used if multiple implementations are possible
- [ ] Unit tests in the same file (`#[cfg(test)] mod tests`)
- [ ] No circular dependencies between modules
- [ ] Config merging happens in `main.rs`, not inside the module
- [ ] Error messages are user-friendly and actionable
- [ ] Zero clippy warnings
