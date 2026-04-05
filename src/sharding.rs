use std::hash::{Hash, Hasher};

use crate::adapters::{TestCase, TestRunResult, TestSuite};
use crate::error::{Result, TestxError};
use crate::hash::StableHasher;

/// Sharding mode for distributing tests across CI workers.
#[derive(Debug, Clone)]
pub enum ShardingMode {
    /// Round-robin slice assignment — simple but not stable across test additions.
    Slice { index: usize, total: usize },
    /// Hash-based assignment — deterministic and stable across test additions.
    Hash { index: usize, total: usize },
}

impl ShardingMode {
    /// Parse a partition string like "slice:1/4" or "hash:2/3".
    pub fn parse(s: &str) -> Result<Self> {
        let (mode, spec) = s.split_once(':').ok_or_else(|| TestxError::ConfigError {
            message: format!(
                "Invalid partition format '{}'. Expected 'slice:M/N' or 'hash:M/N'",
                s
            ),
        })?;

        let (m_str, n_str) = spec
            .split_once('/')
            .ok_or_else(|| TestxError::ConfigError {
                message: format!(
                    "Invalid partition spec '{}'. Expected 'M/N' where 1 <= M <= N",
                    spec
                ),
            })?;

        let m: usize = m_str.parse().map_err(|_| TestxError::ConfigError {
            message: format!(
                "Invalid partition index '{}': must be a positive integer",
                m_str
            ),
        })?;

        let n: usize = n_str.parse().map_err(|_| TestxError::ConfigError {
            message: format!(
                "Invalid partition total '{}': must be a positive integer",
                n_str
            ),
        })?;

        if n == 0 {
            return Err(TestxError::ConfigError {
                message: "Partition total must be >= 1".into(),
            });
        }

        if m == 0 || m > n {
            return Err(TestxError::ConfigError {
                message: format!(
                    "Partition index must satisfy 1 <= M <= N, got M={}, N={}",
                    m, n
                ),
            });
        }

        match mode {
            "slice" => Ok(ShardingMode::Slice { index: m, total: n }),
            "hash" => Ok(ShardingMode::Hash { index: m, total: n }),
            other => Err(TestxError::ConfigError {
                message: format!("Unknown partition mode '{}'. Use 'slice' or 'hash'", other),
            }),
        }
    }

    /// Apply sharding to a test run result, keeping only tests in this shard.
    pub fn apply(&self, result: &TestRunResult) -> TestRunResult {
        match self {
            ShardingMode::Slice { index, total } => shard_slice(result, *index, *total),
            ShardingMode::Hash { index, total } => shard_hash(result, *index, *total),
        }
    }

    /// Return a human-readable description.
    pub fn description(&self) -> String {
        match self {
            ShardingMode::Slice { index, total } => {
                format!("slice {}/{}", index, total)
            }
            ShardingMode::Hash { index, total } => {
                format!("hash {}/{}", index, total)
            }
        }
    }

    /// Return the shard index (1-based).
    pub fn index(&self) -> usize {
        match self {
            ShardingMode::Slice { index, .. } | ShardingMode::Hash { index, .. } => *index,
        }
    }

    /// Return the total number of shards.
    pub fn total(&self) -> usize {
        match self {
            ShardingMode::Slice { total, .. } | ShardingMode::Hash { total, .. } => *total,
        }
    }
}

/// Slice-based sharding: flatten all tests, assign round-robin by position.
fn shard_slice(result: &TestRunResult, index: usize, total: usize) -> TestRunResult {
    // Flatten all tests with their suite index
    let all_tests: Vec<(usize, &TestCase)> = result
        .suites
        .iter()
        .enumerate()
        .flat_map(|(si, s)| s.tests.iter().map(move |t| (si, t)))
        .collect();

    // Keep only tests where (position % total) == (index - 1) since index is 1-based
    let bucket = index - 1;
    let mut suite_tests: Vec<Vec<TestCase>> = vec![Vec::new(); result.suites.len()];

    for (i, (suite_idx, test)) in all_tests.iter().enumerate() {
        if i % total == bucket {
            suite_tests[*suite_idx].push((*test).clone());
        }
    }

    let suites: Vec<TestSuite> = result
        .suites
        .iter()
        .enumerate()
        .filter_map(|(i, orig)| {
            if suite_tests[i].is_empty() {
                None
            } else {
                Some(TestSuite {
                    name: orig.name.clone(),
                    tests: std::mem::take(&mut suite_tests[i]),
                })
            }
        })
        .collect();

    TestRunResult {
        suites,
        duration: result.duration,
        raw_exit_code: result.raw_exit_code,
    }
}

/// Hash-based sharding: deterministic assignment based on hash of suite_index+test name.
fn shard_hash(result: &TestRunResult, index: usize, total: usize) -> TestRunResult {
    let bucket = index - 1;
    let mut suite_tests: Vec<Vec<TestCase>> = vec![Vec::new(); result.suites.len()];

    for (si, suite) in result.suites.iter().enumerate() {
        for test in &suite.tests {
            let hash_key = format!("{}::{}::{}", si, suite.name, test.name);
            let mut hasher = StableHasher::new();
            hash_key.hash(&mut hasher);
            let hash_val = hasher.finish();

            if (hash_val as usize) % total == bucket {
                suite_tests[si].push(test.clone());
            }
        }
    }

    let suites: Vec<TestSuite> = result
        .suites
        .iter()
        .enumerate()
        .filter_map(|(i, orig)| {
            if suite_tests[i].is_empty() {
                None
            } else {
                Some(TestSuite {
                    name: orig.name.clone(),
                    tests: std::mem::take(&mut suite_tests[i]),
                })
            }
        })
        .collect();

    TestRunResult {
        suites,
        duration: result.duration,
        raw_exit_code: result.raw_exit_code,
    }
}

/// Compute sharding statistics for display.
pub struct ShardStats {
    pub total_tests: usize,
    pub shard_tests: usize,
    pub skipped_tests: usize,
    pub shard_index: usize,
    pub shard_total: usize,
}

pub fn compute_shard_stats(
    original: &TestRunResult,
    sharded: &TestRunResult,
    mode: &ShardingMode,
) -> ShardStats {
    let total_tests = original.total_tests();
    let shard_tests = sharded.total_tests();

    ShardStats {
        total_tests,
        shard_tests,
        skipped_tests: total_tests.saturating_sub(shard_tests),
        shard_index: mode.index(),
        shard_total: mode.total(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestError, TestStatus};
    use std::time::Duration;

    fn make_test(name: &str) -> TestCase {
        TestCase {
            name: name.to_string(),
            status: TestStatus::Passed,
            duration: Duration::from_millis(10),
            error: None,
        }
    }

    fn make_result(num_suites: usize, tests_per_suite: usize) -> TestRunResult {
        let suites = (0..num_suites)
            .map(|s| TestSuite {
                name: format!("suite_{}", s),
                tests: (0..tests_per_suite)
                    .map(|t| make_test(&format!("test_{}", t)))
                    .collect(),
            })
            .collect();

        TestRunResult {
            suites,
            duration: Duration::from_secs(1),
            raw_exit_code: 0,
        }
    }

    #[test]
    fn parse_slice_valid() {
        let mode = ShardingMode::parse("slice:1/4").unwrap();
        assert!(matches!(mode, ShardingMode::Slice { index: 1, total: 4 }));
    }

    #[test]
    fn parse_hash_valid() {
        let mode = ShardingMode::parse("hash:2/3").unwrap();
        assert!(matches!(mode, ShardingMode::Hash { index: 2, total: 3 }));
    }

    #[test]
    fn parse_invalid_format() {
        assert!(ShardingMode::parse("invalid").is_err());
        assert!(ShardingMode::parse("slice:1").is_err());
        assert!(ShardingMode::parse("slice:0/3").is_err());
        assert!(ShardingMode::parse("slice:4/3").is_err());
        assert!(ShardingMode::parse("slice:1/0").is_err());
        assert!(ShardingMode::parse("unknown:1/3").is_err());
    }

    #[test]
    fn parse_edge_case_single_shard() {
        let mode = ShardingMode::parse("slice:1/1").unwrap();
        assert!(matches!(mode, ShardingMode::Slice { index: 1, total: 1 }));
    }

    #[test]
    fn slice_distributes_tests_evenly() {
        let result = make_result(1, 8);
        let shard1 = ShardingMode::Slice { index: 1, total: 2 }.apply(&result);
        let shard2 = ShardingMode::Slice { index: 2, total: 2 }.apply(&result);

        assert_eq!(shard1.total_tests(), 4);
        assert_eq!(shard2.total_tests(), 4);
    }

    #[test]
    fn slice_all_shards_cover_all_tests() {
        let result = make_result(2, 5); // 10 tests total
        let total_shards = 3;

        let mut all_test_names: Vec<String> = Vec::new();
        for i in 1..=total_shards {
            let shard = ShardingMode::Slice {
                index: i,
                total: total_shards,
            }
            .apply(&result);
            for suite in &shard.suites {
                for test in &suite.tests {
                    all_test_names.push(format!("{}::{}", suite.name, test.name));
                }
            }
        }

        all_test_names.sort();
        let mut expected_names: Vec<String> = result
            .suites
            .iter()
            .flat_map(|s| {
                s.tests
                    .iter()
                    .map(move |t| format!("{}::{}", s.name, t.name))
            })
            .collect();
        expected_names.sort();

        assert_eq!(all_test_names, expected_names);
    }

    #[test]
    fn slice_no_overlap_between_shards() {
        let result = make_result(2, 6); // 12 tests
        let total = 4;

        let mut all: Vec<Vec<String>> = Vec::new();
        for i in 1..=total {
            let shard = ShardingMode::Slice { index: i, total }.apply(&result);
            let names: Vec<String> = shard
                .suites
                .iter()
                .flat_map(|s| {
                    s.tests
                        .iter()
                        .map(move |t| format!("{}::{}", s.name, t.name))
                })
                .collect();
            all.push(names);
        }

        // Check no overlap
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                for name in &all[i] {
                    assert!(!all[j].contains(name), "Overlap found: {}", name);
                }
            }
        }
    }

    #[test]
    fn slice_single_shard_keeps_all() {
        let result = make_result(2, 5);
        let shard = ShardingMode::Slice { index: 1, total: 1 }.apply(&result);
        assert_eq!(shard.total_tests(), result.total_tests());
    }

    #[test]
    fn hash_deterministic() {
        let result = make_result(2, 5);
        let shard1a = ShardingMode::Hash { index: 1, total: 3 }.apply(&result);
        let shard1b = ShardingMode::Hash { index: 1, total: 3 }.apply(&result);

        let names_a: Vec<String> = shard1a
            .suites
            .iter()
            .flat_map(|s| {
                s.tests
                    .iter()
                    .map(move |t| format!("{}::{}", s.name, t.name))
            })
            .collect();
        let names_b: Vec<String> = shard1b
            .suites
            .iter()
            .flat_map(|s| {
                s.tests
                    .iter()
                    .map(move |t| format!("{}::{}", s.name, t.name))
            })
            .collect();

        assert_eq!(names_a, names_b);
    }

    #[test]
    fn hash_all_shards_cover_all_tests() {
        let result = make_result(3, 4); // 12 tests
        let total = 3;

        let mut all_names: Vec<String> = Vec::new();
        for i in 1..=total {
            let shard = ShardingMode::Hash { index: i, total }.apply(&result);
            for suite in &shard.suites {
                for test in &suite.tests {
                    all_names.push(format!("{}::{}", suite.name, test.name));
                }
            }
        }

        all_names.sort();
        let mut expected: Vec<String> = result
            .suites
            .iter()
            .flat_map(|s| {
                s.tests
                    .iter()
                    .map(move |t| format!("{}::{}", s.name, t.name))
            })
            .collect();
        expected.sort();

        assert_eq!(all_names, expected);
    }

    #[test]
    fn hash_no_overlap() {
        let result = make_result(2, 6);
        let total = 4;

        let mut all: Vec<Vec<String>> = Vec::new();
        for i in 1..=total {
            let shard = ShardingMode::Hash { index: i, total }.apply(&result);
            let names: Vec<String> = shard
                .suites
                .iter()
                .flat_map(|s| {
                    s.tests
                        .iter()
                        .map(move |t| format!("{}::{}", s.name, t.name))
                })
                .collect();
            all.push(names);
        }

        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                for name in &all[i] {
                    assert!(!all[j].contains(name), "Hash overlap: {}", name);
                }
            }
        }
    }

    #[test]
    fn empty_result_sharding() {
        let result = TestRunResult {
            suites: vec![],
            duration: Duration::ZERO,
            raw_exit_code: 0,
        };

        let shard = ShardingMode::Slice { index: 1, total: 3 }.apply(&result);
        assert_eq!(shard.total_tests(), 0);

        let shard = ShardingMode::Hash { index: 1, total: 3 }.apply(&result);
        assert_eq!(shard.total_tests(), 0);
    }

    #[test]
    fn shard_stats_computation() {
        let result = make_result(2, 5);
        let mode = ShardingMode::Slice { index: 1, total: 3 };
        let sharded = mode.apply(&result);
        let stats = compute_shard_stats(&result, &sharded, &mode);

        assert_eq!(stats.total_tests, 10);
        assert_eq!(stats.shard_index, 1);
        assert_eq!(stats.shard_total, 3);
        assert_eq!(stats.shard_tests + stats.skipped_tests, stats.total_tests);
    }

    #[test]
    fn description_format() {
        let slice = ShardingMode::Slice { index: 2, total: 5 };
        assert_eq!(slice.description(), "slice 2/5");

        let hash = ShardingMode::Hash { index: 1, total: 3 };
        assert_eq!(hash.description(), "hash 1/3");
    }

    #[test]
    fn preserves_suite_ordering() {
        let result = make_result(3, 3);
        let shard = ShardingMode::Slice { index: 1, total: 1 }.apply(&result);

        let original_order: Vec<&str> = result.suites.iter().map(|s| s.name.as_str()).collect();
        let shard_order: Vec<&str> = shard.suites.iter().map(|s| s.name.as_str()).collect();

        assert_eq!(original_order, shard_order);
    }

    #[test]
    fn hash_stable_after_test_addition() {
        // Hash sharding should be stable: adding a test shouldn't move existing tests
        let mut result1 = make_result(1, 5);
        let shard1 = ShardingMode::Hash { index: 1, total: 2 }.apply(&result1);
        let names1: Vec<String> = shard1
            .suites
            .iter()
            .flat_map(|s| s.tests.iter().map(move |t| t.name.clone()))
            .collect();

        // Add a new test
        result1.suites[0].tests.push(make_test("test_new"));
        let shard2 = ShardingMode::Hash { index: 1, total: 2 }.apply(&result1);
        let names2: Vec<String> = shard2
            .suites
            .iter()
            .flat_map(|s| s.tests.iter().map(move |t| t.name.clone()))
            .collect();

        // All original tests that were in shard 1 should still be there
        for name in &names1 {
            assert!(
                names2.contains(name),
                "Test '{}' moved after addition",
                name
            );
        }
    }

    #[test]
    fn failed_tests_preserved_in_shard() {
        let mut result = make_result(1, 4);
        result.suites[0].tests[1].status = TestStatus::Failed;
        result.suites[0].tests[1].error = Some(TestError {
            message: "assertion failed".to_string(),
            location: Some("test.rs:42".to_string()),
        });

        let shard = ShardingMode::Slice { index: 1, total: 1 }.apply(&result);
        let failed: Vec<&TestCase> = shard.suites[0]
            .tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .collect();

        assert_eq!(failed.len(), 1);
        assert_eq!(
            failed[0].error.as_ref().unwrap().message,
            "assertion failed"
        );
    }

    #[test]
    fn many_shards_more_than_tests() {
        let result = make_result(1, 3); // 3 tests
        let total = 10; // 10 shards

        let mut total_assigned = 0;
        for i in 1..=total {
            let shard = ShardingMode::Slice { index: i, total }.apply(&result);
            total_assigned += shard.total_tests();
        }

        assert_eq!(total_assigned, 3);
    }
}
