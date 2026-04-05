# Supported Languages

testx supports **11 programming languages** out of the box. It auto-detects your project's language and test framework by scanning for known project files — no configuration needed.

---

## How detection works

When you run `testx`, it scans the current directory for files that indicate a specific language and test framework (like `Cargo.toml` for Rust, `package.json` for JavaScript, etc.).

To see what testx detects without running tests:

```bash
testx detect
```

Output:

```
Detected: Python (pytest) — confidence 94%
```

### Multiple frameworks

If multiple frameworks are detected (e.g., a project with both Rust and Python), testx picks the one with the **highest confidence**. Use `--verbose` to see all candidates and their confidence scores:

```bash
testx detect -v
```

### How confidence is calculated

Confidence isn't a fixed number — it's computed dynamically from weighted signals:

- **Config files** — `Cargo.toml`, `package.json`, `pytest.ini`, etc.
- **Test directories** — `tests/`, `spec/`, `test/`
- **Lock files** — `Cargo.lock`, `package-lock.json`, `go.sum`
- **Runner availability** — Is `cargo`, `pytest`, `jest`, etc. installed and on PATH?

The more signals present, the higher the confidence. This means testx is very accurate in typical projects.

---

## Languages

### Rust

|             |                                                              |
| ----------- | ------------------------------------------------------------ |
| **Trigger** | `Cargo.toml`                                                 |
| **Command** | `cargo test`                                                 |
| **Signals** | `tests/` directory, `Cargo.lock`, `cargo` on PATH, `src/` directory |

testx detects Rust projects by the presence of `Cargo.toml`. It runs `cargo test` and parses the output for pass/fail/skip counts.

### Go

|             |                                                  |
| ----------- | ------------------------------------------------ |
| **Trigger** | `go.mod` with `*_test.go` files present          |
| **Command** | `go test -v ./...`                               |
| **Signals** | `*_test.go` files found, `go.sum`, `go` on PATH |

Go detection requires both `go.mod` and at least one `*_test.go` file. The `-v` flag is added automatically so testx can parse individual test names.

### Python

|                      |                                                                                               |
| -------------------- | --------------------------------------------------------------------------------------------- |
| **Trigger**          | `pytest.ini`, `conftest.py`, `pyproject.toml` with `[tool.pytest]`, or `test_*.py` files      |
| **Command**          | `pytest` (preferred) or `python -m unittest`                                                  |
| **Signals**          | pytest config markers, Django markers, `tests/` directory, lock files, runner on PATH         |
| **Package managers** | uv, poetry, pdm, virtualenv (`.venv`, `venv`)                                                 |

testx prefers pytest over unittest when both are available. It also detects which package manager you're using (uv, poetry, pdm, or pip) and uses the appropriate runner prefix.

### JavaScript / TypeScript

|                      |                                                                         |
| -------------------- | ----------------------------------------------------------------------- |
| **Trigger**          | `vitest.config.*`, `jest.config.*`, `package.json`                      |
| **Command**          | Depends on framework: vitest, jest, bun test, mocha                     |
| **Signals**          | Framework config files, `node_modules/`, lock files, runner on PATH     |
| **Package managers** | bun, pnpm, yarn, npm                                                     |

testx supports multiple JS/TS test frameworks and detects which one you're using based on config files and dependencies. It also detects your package manager (bun > pnpm > yarn > npm) and uses it to run tests.

### Java / Kotlin

|             |                                                                        |
| ----------- | ---------------------------------------------------------------------- |
| **Trigger** | `pom.xml` (Maven) or `build.gradle` / `build.gradle.kts` (Gradle)     |
| **Command** | `mvn test` or `gradle test`                                           |

Maven and Gradle are both supported. testx detects which build system you use and runs the appropriate command.

### C / C++

|             |                                        |
| ----------- | -------------------------------------- |
| **Trigger** | `CMakeLists.txt` or `meson.build`      |
| **Command** | `ctest` or `meson test`                |

For CMake projects, testx runs `ctest`. For Meson projects, it runs `meson test`. Both are detected from their respective build files.

### Ruby

|             |                                                                      |
| ----------- | -------------------------------------------------------------------- |
| **Trigger** | `Gemfile` with rspec/minitest, or `spec/` / `test/` directories     |
| **Command** | `rspec` or `ruby -Itest`                                             |

testx prefers RSpec when a `spec/` directory exists. Falls back to minitest with a `test/` directory.

### Elixir

|             |                |
| ----------- | -------------- |
| **Trigger** | `mix.exs`      |
| **Command** | `mix test`     |

Detected from the Mix build file.

### PHP

|             |                                                              |
| ----------- | ------------------------------------------------------------ |
| **Trigger** | `phpunit.xml` or `composer.json` with phpunit dependency     |
| **Command** | `./vendor/bin/phpunit`                                       |

testx looks for PHPUnit configuration or the phpunit dependency in `composer.json`.

### C# / .NET / F#

|             |                                              |
| ----------- | -------------------------------------------- |
| **Trigger** | `*.csproj`, `*.fsproj`, or `*.sln` files     |
| **Command** | `dotnet test`                                |

Any .NET project file or solution file triggers detection.

### Zig

|             |                      |
| ----------- | -------------------- |
| **Trigger** | `build.zig`          |
| **Command** | `zig build test`     |

Detected from the Zig build file.

---

## Custom adapters

If your project uses a framework testx doesn't natively support, you can define a **custom adapter** in `testx.toml` or globally in `~/.config/testx/adapters/*.toml`. See the [Plugins](../guide/plugins.md) guide for details.

To see all registered adapters (built-in + custom):

```bash
testx adapters
```
