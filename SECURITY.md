# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in testx, please report it responsibly.

**Do NOT file a public issue for security vulnerabilities.**

Instead, please email security concerns to the maintainers directly or use GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability) feature.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgement**: Within 48 hours
- **Assessment**: Within 1 week
- **Fix**: Depending on severity, within 1-4 weeks

### Scope

testx executes test runner commands on the user's system. Security concerns include:

- Command injection through configuration files
- Path traversal in file operations
- Unsafe handling of test output
- Dependency vulnerabilities

## Security Practices

- All user inputs are validated before use in commands
- File paths are canonicalized before access
- Dependencies are regularly audited with `cargo audit`
- CI includes security scanning
