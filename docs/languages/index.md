# Supported Languages

testx auto-detects your project's language and test framework by scanning for known project files.

## Detection

```bash
testx detect
```

If multiple frameworks are detected (e.g., a project with both Rust and Python), testx picks the one with the highest confidence. Use `--verbose` to see all candidates.

## Languages

### Rust

|                |              |
| -------------- | ------------ |
| **Trigger**    | `Cargo.toml` |
| **Command**    | `cargo test` |
| **Confidence** | 0.95         |

### Go

|                |                              |
| -------------- | ---------------------------- |
| **Trigger**    | `go.mod` + `*_test.go` files |
| **Command**    | `go test -v ./...`           |
| **Confidence** | 0.95                         |

### Python

|                      |                                                                                          |
| -------------------- | ---------------------------------------------------------------------------------------- |
| **Trigger**          | `pytest.ini`, `conftest.py`, `pyproject.toml` with `[tool.pytest]`, or `test_*.py` files |
| **Command**          | `pytest` (preferred) or `python -m unittest`                                             |
| **Confidence**       | 0.95 (pytest) / 0.7 (unittest fallback)                                                  |
| **Package managers** | uv, poetry, pdm, virtualenv (`.venv`, `venv`)                                            |

### JavaScript / TypeScript

|                      |                                                     |
| -------------------- | --------------------------------------------------- |
| **Trigger**          | `vitest.config.*`, `jest.config.*`, `package.json`  |
| **Command**          | Depends on framework: vitest, jest, bun test, mocha |
| **Confidence**       | 0.9                                                 |
| **Package managers** | bun, pnpm, yarn, npm                                |

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

You can define additional frameworks in `testx.toml`. See [Plugins](../guide/plugins.md).
