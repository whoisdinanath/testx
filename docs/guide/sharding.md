# CI Sharding

When your test suite grows large, running all tests on a single CI node can become a bottleneck. **Sharding** splits your test suite across multiple CI nodes so they run in parallel, reducing total wall-clock time.

For example, if you have 1,000 tests and 4 CI nodes, each node runs ~250 tests simultaneously — completing in roughly 1/4 the time.

---

## Usage

```bash
testx --partition <strategy>:<index>/<total>
```

- `strategy` — How to split tests: `slice` or `hash`
- `index` — Which shard this node handles (starting from 1)
- `total` — Total number of shards

---

## Strategies

### Slice (ordered)

Splits tests into sequential chunks. The first shard gets the first quarter, the second shard gets the second quarter, etc.

```bash
testx --partition slice:1/4   # Tests 1–250
testx --partition slice:2/4   # Tests 251–500
testx --partition slice:3/4   # Tests 501–750
testx --partition slice:4/4   # Tests 751–1000
```

**Pros:** Simple, deterministic — same tests always go to the same shard.
**Cons:** Adding or removing tests shuffles which tests land on which shard.

### Hash (stable)

Assigns tests to shards by hashing the test name. This means each test always goes to the same shard, even when tests are added or removed elsewhere.

```bash
testx --partition hash:1/3
testx --partition hash:2/3
testx --partition hash:3/3
```

**Pros:** Stable — existing tests don't move between shards when you add new tests.
**Cons:** Shard sizes may be slightly uneven (hash distribution isn't perfectly uniform).

**Which to choose?** Use `hash` if your test suite changes frequently (adding/removing tests). Use `slice` for simpler setups where stability doesn't matter.

---

## CI examples

### GitHub Actions

```yaml
jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2, 3, 4]
    steps:
      - uses: actions/checkout@v4
      - name: Run tests (shard ${{ matrix.shard }}/4)
        run: testx --partition slice:${{ matrix.shard }}/4
```

This creates 4 parallel jobs, each running 1/4 of your tests.

### GitLab CI

```yaml
test:
  parallel: 4
  script:
    - testx --partition slice:${CI_NODE_INDEX}/${CI_NODE_TOTAL}
```

GitLab automatically sets `CI_NODE_INDEX` and `CI_NODE_TOTAL` when using `parallel`.

---

## Combining with other features

Sharding works well with caching and impact analysis for maximum speed:

```bash
# Skip unchanged shards entirely
testx --affected=branch:main --partition slice:1/4

# Use cached results when nothing changed
testx --cache --partition hash:1/3
```

---

## Guarantees

- All shards combined cover **100% of tests** — nothing is skipped or duplicated
- No overlap — each test runs on exactly one shard
- Both strategies are deterministic — same input produces the same split every time
