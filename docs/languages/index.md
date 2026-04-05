# Supported Languages

testx auto-detects your project's language and test framework by scanning for known project files.

## Detection

```bash
testx detect
```

If multiple frameworks are detected (e.g., a project with both Rust and Python), testx picks the one with the highest confidence. Use `--verbose` to see all candidates.

Confidence is computed dynamically from weighted signals — config files, test directories, lock files, and runner availability — rather than a fixed number. More signals present means higher confidence.

## Languages

### Rust

|             |                                                         |
| ----------- | ------------------------------------------------------- |
| **Trigger** | `Cargo.toml`                                            |
| **Command** | `cargo test`                                            |
| **Signals** | `tests/` dir, `Cargo.lock`, `cargo` on PATH, `src/` dir |

### Go

|             |                                          |
| ----------- | ---------------------------------------- |
| **Trigger** | `go.mod` + `*_test.go` files             |
| **Command** | `go test -v ./...`                       |
| **Signals** | test files found, `go.sum`, `go` on PATH |

### Python

|                      |                                                                                          |
| -------------------- | ---------------------------------------------------------------------------------------- |
| **Trigger**          | `pytest.ini`, `conftest.py`, `pyproject.toml` with `[tool.pytest]`, or `test_*.py` files |
| **Command**          | `pytest` (preferred) or `python -m unittest`                                             |
| **Signals**          | pytest/django markers, `tests/` dir, lock files, runner on PATH                          |
| **Package managers** | uv, poetry, pdm, virtualenv (`.venv`, `venv`)                                            |

### JavaScript / TypeScript

|                      |                                                                     |
| -------------------- | ------------------------------------------------------------------- |
| **Trigger**          | `vitest.config.*`, `jest.config.*`, `package.json`                  |
| **Command**          | Depends on framework: vitest, jest, bun test, mocha                 |
| **Signals**          | framework config files, `node_modules/`, lock files, runner on PATH |
| **Package managers** | bun, pnpm, yarn, npm                                                |

### Java / Kotlin

|             |                                                                   |
| ----------- | ----------------------------------------------------------------- |
| **Trigger** | `pom.xml` (Maven) or `build.gradle` / `build.gradle.kts` (Gradle) |
| **Command** | `mvn test` or `gradle test`                                       |

### C / C++

|             |                                   |
| ----------- | --------------------------------- |
| **Trigger** | `CMakeLists.txt` or `meson.build` |
| **Command** | `ctest` or `meson test`           |

### Ruby

|             |                                                                 |
| ----------- | --------------------------------------------------------------- |
| **Trigger** | `Gemfile` with rspec/minitest, or `spec/` / `test/` directories |
| **Command** | `rspec` or `ruby -Itest`                                        |

### Elixir

|             |            |
| ----------- | ---------- |
| **Trigger** | `mix.exs`  |
| **Command** | `mix test` |

### PHP

|             |                                                          |
| ----------- | -------------------------------------------------------- |
| **Trigger** | `phpunit.xml` or `composer.json` with phpunit dependency |
| **Command** | `./vendor/bin/phpunit`                                   |

### C# / .NET / F#

|             |                                          |
| ----------- | ---------------------------------------- |
| **Trigger** | `*.csproj`, `*.fsproj`, or `*.sln` files |
| **Command** | `dotnet test`                            |

### Zig

|             |                  |
| ----------- | ---------------- |
| **Trigger** | `build.zig`      |
| **Command** | `zig build test` |

## Custom adapters

You can define additional frameworks in `testx.toml` or globally in `~/.config/testx/adapters/*.toml`. See [Plugins](../guide/plugins.md) for details.

List all registered adapters (built-in + custom):

```bash
testx adapters
```
