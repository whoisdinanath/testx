//! Analytics and statistics module.
//!
//! Provides aggregate analysis over test history data,
//! including health scores, failure correlations, and
//! performance monitoring.

use std::collections::HashMap;
use std::fmt::Write;

use super::{RunRecord, TestHistory};

/// Overall test suite health score (0-100).
#[derive(Debug, Clone)]
pub struct HealthScore {
    /// Overall score (0-100)
    pub score: f64,
    /// Pass rate component (0-100)
    pub pass_rate: f64,
    /// Stability component (0-100) — low flakiness
    pub stability: f64,
    /// Performance component (0-100) — consistent durations
    pub performance: f64,
    /// Coverage component (0-100) — if available
    pub coverage: Option<f64>,
}

impl HealthScore {
    /// Compute a health score from test history.
    pub fn compute(history: &TestHistory) -> Self {
        let recent = history.recent_runs(30);
        if recent.is_empty() {
            return Self {
                score: 0.0,
                pass_rate: 0.0,
                stability: 100.0,
                performance: 100.0,
                coverage: None,
            };
        }

        let pass_rate = compute_pass_rate(recent);
        let stability = compute_stability(recent);
        let performance = compute_performance_score(recent);

        // Weighted average: pass_rate 50%, stability 30%, performance 20%
        let score = pass_rate * 0.5 + stability * 0.3 + performance * 0.2;

        Self {
            score,
            pass_rate,
            stability,
            performance,
            coverage: None,
        }
    }

    /// Get a letter grade for the score.
    pub fn grade(&self) -> &str {
        match self.score as u32 {
            90..=100 => "A",
            80..=89 => "B",
            70..=79 => "C",
            60..=69 => "D",
            _ => "F",
        }
    }

    /// Get a color indicator for the score.
    pub fn indicator(&self) -> &str {
        if self.score >= 90.0 {
            "🟢"
        } else if self.score >= 70.0 {
            "🟡"
        } else {
            "🔴"
        }
    }
}

/// Compute pass rate as a 0-100 score.
fn compute_pass_rate(runs: &[RunRecord]) -> f64 {
    let total_passed: usize = runs.iter().map(|r| r.passed).sum();
    let total_tests: usize = runs.iter().map(|r| r.total).sum();

    if total_tests > 0 {
        total_passed as f64 / total_tests as f64 * 100.0
    } else {
        0.0
    }
}

/// Compute stability score (inversely proportional to flakiness).
fn compute_stability(runs: &[RunRecord]) -> f64 {
    if runs.len() < 2 {
        return 100.0;
    }

    // Count status transitions per test
    let mut test_results: HashMap<String, Vec<bool>> = HashMap::new();
    for run in runs {
        for test in &run.tests {
            test_results
                .entry(test.name.clone())
                .or_default()
                .push(test.status == "passed");
        }
    }

    let mut total_transitions = 0usize;
    let mut total_comparisons = 0usize;

    for results in test_results.values() {
        if results.len() < 2 {
            continue;
        }
        for window in results.windows(2) {
            total_comparisons += 1;
            if window[0] != window[1] {
                total_transitions += 1;
            }
        }
    }

    if total_comparisons == 0 {
        return 100.0;
    }

    let transition_rate = total_transitions as f64 / total_comparisons as f64;
    // Convert rate to score: 0 transitions = 100%, 50% transitions = 0%
    (1.0 - transition_rate * 2.0).max(0.0) * 100.0
}

/// Compute performance consistency score.
fn compute_performance_score(runs: &[RunRecord]) -> f64 {
    if runs.len() < 3 {
        return 100.0;
    }

    let durations: Vec<f64> = runs.iter().map(|r| r.duration_ms as f64).collect();
    let mean = durations.iter().sum::<f64>() / durations.len() as f64;

    if mean == 0.0 {
        return 100.0;
    }

    // Coefficient of variation (lower = more consistent)
    let variance =
        durations.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / durations.len() as f64;
    let std_dev = variance.sqrt();
    let cv = std_dev / mean;

    // Score: CV of 0 = 100, CV of 1.0+ = 0
    (1.0 - cv).max(0.0) * 100.0
}

/// Failure correlation analysis.
#[derive(Debug, Clone)]
pub struct FailureCorrelation {
    /// Tests that tend to fail together
    pub pairs: Vec<CorrelatedPair>,
}

/// A pair of tests that frequently fail together.
#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub test_a: String,
    pub test_b: String,
    /// How often they fail together vs individually (0.0 - 1.0)
    pub correlation: f64,
    /// Number of co-failures
    pub co_failures: usize,
}

impl FailureCorrelation {
    /// Compute failure correlations from history.
    pub fn compute(history: &TestHistory, min_cooccurrences: usize) -> Self {
        let recent = history.recent_runs(50);
        let mut failure_sets: Vec<Vec<String>> = Vec::new();

        for run in recent {
            let failures: Vec<String> = run
                .tests
                .iter()
                .filter(|t| t.status == "failed")
                .map(|t| t.name.clone())
                .collect();
            if !failures.is_empty() {
                failure_sets.push(failures);
            }
        }

        let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
        let mut individual_counts: HashMap<String, usize> = HashMap::new();

        for failures in &failure_sets {
            for test in failures {
                *individual_counts.entry(test.clone()).or_default() += 1;
            }

            for i in 0..failures.len() {
                for j in (i + 1)..failures.len() {
                    let (a, b) = if failures[i] < failures[j] {
                        (failures[i].clone(), failures[j].clone())
                    } else {
                        (failures[j].clone(), failures[i].clone())
                    };
                    *pair_counts.entry((a, b)).or_default() += 1;
                }
            }
        }

        let mut pairs = Vec::new();
        for ((a, b), count) in &pair_counts {
            if *count < min_cooccurrences {
                continue;
            }

            let a_count = individual_counts.get(a).copied().unwrap_or(0);
            let b_count = individual_counts.get(b).copied().unwrap_or(0);
            let max_individual = a_count.max(b_count);

            let correlation = if max_individual > 0 {
                *count as f64 / max_individual as f64
            } else {
                0.0
            };

            pairs.push(CorrelatedPair {
                test_a: a.clone(),
                test_b: b.clone(),
                correlation,
                co_failures: *count,
            });
        }

        pairs.sort_by(|a, b| {
            b.correlation
                .partial_cmp(&a.correlation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        FailureCorrelation { pairs }
    }
}

/// Format the analytics dashboard.
pub fn format_analytics_dashboard(history: &TestHistory) -> String {
    let mut out = String::with_capacity(2048);
    let health = HealthScore::compute(history);

    let _ = writeln!(out);
    let _ = writeln!(out, "  Test Analytics Dashboard");
    let _ = writeln!(out, "  ═══════════════════════════════════════");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  Health Score: {} {:.0}/100 ({})",
        health.indicator(),
        health.score,
        health.grade()
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "  Components:");
    let _ = writeln!(
        out,
        "    Pass Rate:    {} {:.1}%",
        score_bar(health.pass_rate),
        health.pass_rate
    );
    let _ = writeln!(
        out,
        "    Stability:    {} {:.1}%",
        score_bar(health.stability),
        health.stability
    );
    let _ = writeln!(
        out,
        "    Performance:  {} {:.1}%",
        score_bar(health.performance),
        health.performance
    );
    if let Some(cov) = health.coverage {
        let _ = writeln!(out, "    Coverage:     {} {:.1}%", score_bar(cov), cov);
    }
    let _ = writeln!(out);

    // Run stats
    let _ = writeln!(out, "  Run Statistics:");
    let _ = writeln!(out, "    Total Runs:   {}", history.run_count());
    let _ = writeln!(
        out,
        "    Avg Duration: {}",
        format_duration_ms(history.avg_duration(30).as_millis() as u64)
    );

    let recent = history.recent_runs(30);
    let total_failures: usize = recent.iter().map(|r| r.failed).sum();
    let _ = writeln!(out, "    Total Fails:  {} (last 30 runs)", total_failures);

    // Failure correlation
    let correlations = FailureCorrelation::compute(history, 2);
    if !correlations.pairs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Correlated Failures:");
        for pair in correlations.pairs.iter().take(5) {
            let _ = writeln!(
                out,
                "    {:.0}% {} ↔ {} ({} co-failures)",
                pair.correlation * 100.0,
                truncate_name(&pair.test_a, 25),
                truncate_name(&pair.test_b, 25),
                pair.co_failures,
            );
        }
    }

    let _ = writeln!(out);
    out
}

/// Create a score bar (5 characters wide).
fn score_bar(score: f64) -> String {
    let filled = ((score / 100.0) * 5.0).round() as usize;
    let filled = filled.min(5);
    let empty = 5 - filled;
    format!("│{}{}│", "█".repeat(filled), "░".repeat(empty))
}

/// Truncate a test name to max characters.
fn truncate_name(name: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if name.len() <= max {
        name.to_string()
    } else {
        let start = name.ceil_char_boundary(name.len().saturating_sub(max - 1));
        format!("…{}", &name[start..])
    }
}

fn format_duration_ms(ms: u64) -> String {
    if ms == 0 {
        "<1ms".to_string()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};
    use crate::history::RunRecord;
    use std::time::Duration;

    fn make_result(passed: usize, failed: usize) -> TestRunResult {
        let mut tests = Vec::new();
        for i in 0..passed {
            tests.push(TestCase {
                name: format!("pass_{i}"),
                status: TestStatus::Passed,
                duration: Duration::from_millis(10),
                error: None,
            });
        }
        for i in 0..failed {
            tests.push(TestCase {
                name: format!("fail_{i}"),
                status: TestStatus::Failed,
                duration: Duration::from_millis(5),
                error: None,
            });
        }
        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".into(),
                tests,
            }],
            duration: Duration::from_millis(100),
            raw_exit_code: if failed > 0 { 1 } else { 0 },
        }
    }

    fn populated_history() -> TestHistory {
        let mut h = TestHistory::new_in_memory();
        for _ in 0..10 {
            h.runs.push(RunRecord::from_result(&make_result(5, 0)));
        }
        h
    }

    #[test]
    fn health_score_all_pass() {
        let h = populated_history();
        let score = HealthScore::compute(&h);
        assert!(score.score > 90.0);
        assert_eq!(score.grade(), "A");
        assert_eq!(score.indicator(), "🟢");
    }

    #[test]
    fn health_score_empty() {
        let h = TestHistory::new_in_memory();
        let score = HealthScore::compute(&h);
        assert_eq!(score.score, 0.0); // No runs → score 0
    }

    #[test]
    fn health_score_with_failures() {
        let mut h = TestHistory::new_in_memory();
        for _ in 0..10 {
            h.runs.push(RunRecord::from_result(&make_result(3, 2)));
        }
        let score = HealthScore::compute(&h);
        assert!(score.pass_rate < 70.0);
        assert!(score.score <= 80.0);
    }

    #[test]
    fn stability_no_transitions() {
        let mut h = TestHistory::new_in_memory();
        for _ in 0..5 {
            h.runs.push(RunRecord::from_result(&make_result(5, 0)));
        }
        let score = HealthScore::compute(&h);
        assert_eq!(score.stability, 100.0);
    }

    #[test]
    fn stability_with_transitions() {
        let mut h = TestHistory::new_in_memory();
        // Use the same test name alternating between pass and fail
        for i in 0..10 {
            let status = if i % 2 == 0 {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            let result = TestRunResult {
                suites: vec![TestSuite {
                    name: "suite".into(),
                    tests: vec![TestCase {
                        name: "alternating_test".into(),
                        status,
                        duration: Duration::from_millis(10),
                        error: None,
                    }],
                }],
                duration: Duration::from_millis(100),
                raw_exit_code: 0,
            };
            h.runs.push(RunRecord::from_result(&result));
        }
        let score = HealthScore::compute(&h);
        assert!(score.stability < 50.0);
    }

    #[test]
    fn grade_boundaries() {
        let s = |score: f64| {
            let h = HealthScore {
                score,
                pass_rate: score,
                stability: 100.0,
                performance: 100.0,
                coverage: None,
            };
            h.grade().to_string()
        };
        assert_eq!(s(95.0), "A");
        assert_eq!(s(85.0), "B");
        assert_eq!(s(75.0), "C");
        assert_eq!(s(65.0), "D");
        assert_eq!(s(50.0), "F");
    }

    #[test]
    fn analytics_dashboard() {
        let h = populated_history();
        let output = format_analytics_dashboard(&h);
        assert!(output.contains("Test Analytics Dashboard"));
        assert!(output.contains("Health Score"));
        assert!(output.contains("Pass Rate"));
        assert!(output.contains("Run Statistics"));
    }

    #[test]
    fn score_bar_full() {
        let bar = score_bar(100.0);
        assert!(bar.contains("█████"));
    }

    #[test]
    fn score_bar_empty() {
        let bar = score_bar(0.0);
        assert!(bar.contains("░░░░░"));
    }

    #[test]
    fn failure_correlation_empty() {
        let h = populated_history();
        let corr = FailureCorrelation::compute(&h, 2);
        assert!(corr.pairs.is_empty());
    }

    #[test]
    fn failure_correlation_detected() {
        let mut h = TestHistory::new_in_memory();
        // Create runs where fail_0 and fail_1 always fail together
        for _ in 0..5 {
            h.runs.push(RunRecord::from_result(&make_result(3, 2)));
        }
        let corr = FailureCorrelation::compute(&h, 2);
        assert!(!corr.pairs.is_empty());
        assert!(corr.pairs[0].correlation > 0.5);
    }

    #[test]
    fn truncate_name_short() {
        assert_eq!(truncate_name("short", 10), "short");
    }

    #[test]
    fn truncate_name_long() {
        let truncated = truncate_name("very_long_test_name_that_exceeds", 15);
        assert!(truncated.starts_with('…'));
        assert_eq!(truncated.chars().count(), 15);
    }

    #[test]
    fn performance_score_consistent() {
        let recent: Vec<RunRecord> = (0..5)
            .map(|_| {
                let mut r = RunRecord::from_result(&make_result(5, 0));
                r.duration_ms = 100;
                r
            })
            .collect();
        let score = compute_performance_score(&recent);
        assert_eq!(score, 100.0);
    }

    #[test]
    fn performance_score_variable() {
        let recent: Vec<RunRecord> = [100, 500, 100, 500, 100]
            .iter()
            .map(|&ms| {
                let mut r = RunRecord::from_result(&make_result(5, 0));
                r.duration_ms = ms;
                r
            })
            .collect();
        let score = compute_performance_score(&recent);
        assert!(score < 100.0);
    }
}
