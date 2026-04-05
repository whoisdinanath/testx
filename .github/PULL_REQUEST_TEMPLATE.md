## Description

<!-- What does this PR do? Keep it concise. Link to an issue if applicable. -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] New adapter (adding support for a new language/framework)
- [ ] Documentation update
- [ ] Refactor (no functional changes)
- [ ] Performance improvement

## How Has This Been Tested?

<!-- Describe the tests you ran. Include OS, Rust version, and relevant project details. -->

- [ ] Unit tests (`cargo test`)
- [ ] Integration tests (`cargo test --test integration`)
- [ ] CLI tests (`cargo test --test cli`)
- [ ] Manual testing with a real project (language/framework: \_\_\_)

## Quality Checklist

- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] Code is formatted (`cargo fmt --all -- --check`)
- [ ] New code follows the [Coding Guidelines](.github/CODING_GUIDELINES.md)
- [ ] Public APIs have doc comments (`///`)
- [ ] CHANGELOG.md updated (if user-facing change)

### Safety Checks (if applicable)

- [ ] Recursive functions have depth limits (max depth constant)
- [ ] Filesystem traversal has symlink loop protection (`visited` HashSet + `canonicalize()`)
- [ ] No unbounded `Vec` growth in hot loops
- [ ] Process exit codes are handled — no assumption of success

### For New Adapters

- [ ] Implements all `TestAdapter` trait methods
- [ ] Detection tests (positive and negative)
- [ ] Command building tests
- [ ] Output parsing tests (pass, fail, skip, error, empty)
- [ ] `check_runner()` verifies the test runner binary exists
- [ ] Registered in `DetectionEngine::new()`
- [ ] README.md updated with the new language

## Related Issues

<!-- Use "Closes #123" to auto-close an issue when this PR merges -->

## Screenshots / Output (if applicable)

<!-- Paste terminal output or screenshots showing the change in action -->
