# CI Sharding

Split your test suite across multiple CI nodes for faster parallel runs.

## Usage

```bash
testx --partition slice:<index>/<total>
testx --partition hash:<index>/<total>
```

## Strategies

### Slice (ordered)

Splits tests into sequential chunks. Deterministic — same tests always go to the same shard.

```bash
testx --partition slice:1/4   # First quarter
testx --partition slice:2/4   # Second quarter
testx --partition slice:3/4   # Third quarter
testx --partition slice:4/4   # Last quarter
```

### Hash (stable)

Assigns tests to shards by hashing the test name. When you add or remove tests, existing tests stay on the same shard.

```bash
testx --partition hash:1/3
testx --partition hash:2/3
testx --partition hash:3/3
```

## GitHub Actions example

```yaml
jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2, 3, 4]
    steps:
      - uses: actions/checkout@v4
      - run: testx --partition slice:${{ matrix.shard }}/4
```

## GitLab CI example

```yaml
test:
  parallel: 4
  script:
    - testx --partition slice:${CI_NODE_INDEX}/${CI_NODE_TOTAL}
```

## Notes

- All shards combined cover 100% of tests with no overlap
- Combine with `--cache` to skip unchanged shards
- Combine with `--affected` to skip shards entirely when nothing changed
