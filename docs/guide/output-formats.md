# Output Formats

testx supports four output formats.

## Pretty (default)

```bash
testx
```

Clean, colored terminal output with pass/fail icons and timing.

## JSON

```bash
testx -o json
```

Machine-readable structured output. Useful for scripts and dashboards.

```json
{
  "suites": [
    {
      "name": "tests/test_math.py",
      "tests": [
        { "name": "test_add", "status": "passed", "duration": 0.001 },
        { "name": "test_divide", "status": "failed", "duration": 0.002 }
      ]
    }
  ],
  "duration": 0.12,
  "exit_code": 1
}
```

## JUnit XML

```bash
testx -o junit > test-results.xml
```

Standard JUnit XML format. Compatible with Jenkins, GitLab CI, GitHub Actions, and most CI systems.

## TAP (Test Anything Protocol)

```bash
testx -o tap
```

```
TAP version 13
1..3
ok 1 - test_add
ok 2 - test_subtract
not ok 3 - test_divide_by_zero
```
