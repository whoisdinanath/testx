# Contributing to testx

Thank you for your interest in contributing to testx! This guide will help you get started.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/testx.git`
3. Create a branch: `git checkout -b feature/my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Run lints: `cargo clippy --all-targets`
7. Format code: `cargo fmt`
8. Push and create a Pull Request

## Development Setup

### Prerequisites

- Rust 1.87+ (edition 2024)
- Git

### Building

```sh
cargo build
```

### Running Tests

```sh
# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run benchmarks
cargo bench
```

### Code Quality

```sh
# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all

# Check formatting
cargo fmt --all -- --check
```

## Adding a New Adapter

1. Create `src/adapters/language.rs`
2. Implement the `TestAdapter` trait
3. Register it in `src/detection/mod.rs`
4. Add tests for:
   - Detection (project file matching)
   - Command building
   - Output parsing (pass, fail, skip, errors)
5. Update README.md

### Adapter Trait

```rust
pub trait TestAdapter {
    fn name(&self) -> &str;
    fn detect(&self, dir: &Path) -> Option<Detection>;
    fn build_command(&self, dir: &Path, args: &[String]) -> Result<Command>;
    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult;
    fn check_runner(&self) -> Option<String>;
}
```

## Code Style

- Follow Rust conventions and idioms
- Keep functions focused and small
- Write tests for all new functionality
- Maintain zero clippy warnings
- Use meaningful variable and function names

## Commit Messages

Use conventional commits:

- `feat: add Python adapter`
- `fix: handle empty test output`
- `docs: update README with new flags`
- `test: add integration tests for retry`
- `refactor: extract common parsing logic`

## Pull Request Guidelines

- One feature/fix per PR
- Include tests
- Update documentation if needed
- Ensure CI passes
- Keep PRs reasonably sized

## Reporting Issues

- Use the issue templates
- Include testx version (`testx --version`)
- Include your OS and Rust version
- Provide minimal reproduction steps

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
