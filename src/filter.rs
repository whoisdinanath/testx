use crate::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};

/// A compiled test filter that can match test cases by name pattern.
#[derive(Debug, Clone)]
pub struct TestFilter {
    /// Include patterns — tests must match at least one (if non-empty)
    include: Vec<FilterPattern>,
    /// Exclude patterns — tests matching any of these are removed
    exclude: Vec<FilterPattern>,
    /// Only include tests with these statuses (empty = all)
    status_filter: Vec<TestStatus>,
    /// Filter by suite name
    suite_filter: Option<String>,
}

/// A single filter pattern that can match test names.
#[derive(Debug, Clone)]
pub enum FilterPattern {
    /// Exact string match
    Exact(String),
    /// Prefix match (pattern ends with *)
    Prefix(String),
    /// Suffix match (pattern starts with *)
    Suffix(String),
    /// Contains match (pattern starts and ends with *)
    Contains(String),
    /// Simple glob with * wildcards
    Glob(Vec<GlobSegment>),
}

/// A segment in a glob pattern.
#[derive(Debug, Clone)]
pub enum GlobSegment {
    /// A literal string to match
    Literal(String),
    /// A wildcard matching any characters
    Wildcard,
}

impl TestFilter {
    /// Create a new empty filter (matches everything).
    pub fn new() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            status_filter: Vec::new(),
            suite_filter: None,
        }
    }

    /// Add an include pattern.
    pub fn include(mut self, pattern: &str) -> Self {
        self.include.push(FilterPattern::parse(pattern));
        self
    }

    /// Add multiple include patterns from a comma-separated string.
    pub fn include_csv(mut self, patterns: &str) -> Self {
        for pattern in patterns.split(',') {
            let pattern = pattern.trim();
            if !pattern.is_empty() {
                self.include.push(FilterPattern::parse(pattern));
            }
        }
        self
    }

    /// Add an exclude pattern.
    pub fn exclude(mut self, pattern: &str) -> Self {
        self.exclude.push(FilterPattern::parse(pattern));
        self
    }

    /// Add multiple exclude patterns from a comma-separated string.
    pub fn exclude_csv(mut self, patterns: &str) -> Self {
        for pattern in patterns.split(',') {
            let pattern = pattern.trim();
            if !pattern.is_empty() {
                self.exclude.push(FilterPattern::parse(pattern));
            }
        }
        self
    }

    /// Only show tests with the given status.
    pub fn status(mut self, status: TestStatus) -> Self {
        self.status_filter.push(status);
        self
    }

    /// Only show tests from suites matching this name.
    pub fn suite(mut self, name: &str) -> Self {
        self.suite_filter = Some(name.to_string());
        self
    }

    /// Check if this filter has any active constraints.
    pub fn is_active(&self) -> bool {
        !self.include.is_empty()
            || !self.exclude.is_empty()
            || !self.status_filter.is_empty()
            || self.suite_filter.is_some()
    }

    /// Check if a test case matches this filter.
    pub fn matches(&self, test: &TestCase, suite_name: &str) -> bool {
        // Check suite filter
        if let Some(ref sf) = self.suite_filter
            && !suite_name.contains(sf)
        {
            return false;
        }

        // Check status filter
        if !self.status_filter.is_empty() && !self.status_filter.contains(&test.status) {
            return false;
        }

        // Check exclude patterns first (any match excludes)
        if self.exclude.iter().any(|p| p.matches(&test.name)) {
            return false;
        }

        // Check include patterns (must match at least one, if any exist)
        if !self.include.is_empty() && !self.include.iter().any(|p| p.matches(&test.name)) {
            return false;
        }

        true
    }

    /// Apply this filter to a test run result, returning a filtered copy.
    pub fn apply(&self, result: &TestRunResult) -> TestRunResult {
        if !self.is_active() {
            return result.clone();
        }

        let suites = result
            .suites
            .iter()
            .filter_map(|suite| {
                // Apply suite filter
                if let Some(ref sf) = self.suite_filter
                    && !suite.name.contains(sf)
                {
                    return None;
                }

                let tests: Vec<TestCase> = suite
                    .tests
                    .iter()
                    .filter(|t| self.matches(t, &suite.name))
                    .cloned()
                    .collect();

                if tests.is_empty() {
                    None
                } else {
                    Some(TestSuite {
                        name: suite.name.clone(),
                        tests,
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
}

impl Default for TestFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterPattern {
    /// Parse a string pattern into a FilterPattern.
    pub fn parse(pattern: &str) -> Self {
        if !pattern.contains('*') {
            return FilterPattern::Exact(pattern.to_string());
        }

        // "*foo" -> Suffix
        if pattern.starts_with('*') && !pattern[1..].contains('*') {
            return FilterPattern::Suffix(pattern[1..].to_string());
        }

        // "foo*" -> Prefix
        if pattern.ends_with('*') && !pattern[..pattern.len() - 1].contains('*') {
            return FilterPattern::Prefix(pattern[..pattern.len() - 1].to_string());
        }

        // "*foo*" -> Contains
        if pattern.starts_with('*')
            && pattern.ends_with('*')
            && !pattern[1..pattern.len() - 1].contains('*')
        {
            return FilterPattern::Contains(pattern[1..pattern.len() - 1].to_string());
        }

        // General glob pattern
        let segments = parse_glob_segments(pattern);
        FilterPattern::Glob(segments)
    }

    /// Check if a test name matches this pattern.
    pub fn matches(&self, name: &str) -> bool {
        match self {
            FilterPattern::Exact(s) => name == s,
            FilterPattern::Prefix(p) => name.starts_with(p),
            FilterPattern::Suffix(s) => name.ends_with(s),
            FilterPattern::Contains(s) => name.contains(s),
            FilterPattern::Glob(segments) => glob_match(segments, name),
        }
    }
}

/// Parse a glob pattern into segments.
fn parse_glob_segments(pattern: &str) -> Vec<GlobSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in pattern.chars() {
        if ch == '*' {
            if !current.is_empty() {
                segments.push(GlobSegment::Literal(std::mem::take(&mut current)));
            }
            // Collapse multiple wildcards
            if !matches!(segments.last(), Some(GlobSegment::Wildcard)) {
                segments.push(GlobSegment::Wildcard);
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        segments.push(GlobSegment::Literal(current));
    }

    segments
}

/// Match a glob pattern against a string.
fn glob_match(segments: &[GlobSegment], s: &str) -> bool {
    glob_match_recursive(segments, s, 0, 0, 0)
}

fn glob_match_recursive(
    segments: &[GlobSegment],
    s: &str,
    seg_idx: usize,
    str_idx: usize,
    depth: usize,
) -> bool {
    // Guard against pathological patterns (e.g. *a*b*c*d*... on non-matching strings)
    const MAX_DEPTH: usize = 1024;
    if depth > MAX_DEPTH {
        return false;
    }

    if seg_idx == segments.len() {
        return str_idx == s.len();
    }

    match &segments[seg_idx] {
        GlobSegment::Literal(lit) => {
            if s[str_idx..].starts_with(lit) {
                glob_match_recursive(segments, s, seg_idx + 1, str_idx + lit.len(), depth + 1)
            } else {
                false
            }
        }
        GlobSegment::Wildcard => {
            // Try matching 0 or more characters.
            // Optimization: if this is the last segment, any remaining string matches.
            if seg_idx + 1 == segments.len() {
                return true;
            }
            // Skip ahead to the next literal to avoid exponential backtracking:
            // find all positions where the next literal could start, only recurse there.
            if let Some(GlobSegment::Literal(next_lit)) = segments.get(seg_idx + 1) {
                let remaining = &s[str_idx..];
                let mut search_from = 0;
                while let Some(pos) = remaining[search_from..].find(next_lit.as_str()) {
                    let abs_pos = str_idx + search_from + pos;
                    if glob_match_recursive(segments, s, seg_idx + 1, abs_pos, depth + 1) {
                        return true;
                    }
                    search_from += pos + 1;
                }
                false
            } else {
                // Next segment is also a wildcard — try each position
                for i in str_idx..=s.len() {
                    if glob_match_recursive(segments, s, seg_idx + 1, i, depth + 1) {
                        return true;
                    }
                }
                false
            }
        }
    }
}

/// Build a TestFilter from filter configuration strings.
pub fn build_filter(include: Option<&str>, exclude: Option<&str>, failed_only: bool) -> TestFilter {
    let mut filter = TestFilter::new();

    if let Some(inc) = include {
        filter = filter.include_csv(inc);
    }
    if let Some(exc) = exclude {
        filter = filter.exclude_csv(exc);
    }
    if failed_only {
        filter = filter.status(TestStatus::Failed);
    }

    filter
}

/// Filter test results and return summary statistics.
pub struct FilterSummary {
    /// Total tests before filtering
    pub total_before: usize,
    /// Total tests after filtering
    pub total_after: usize,
    /// Number of tests removed
    pub filtered_out: usize,
    /// Number of suites removed entirely
    pub suites_removed: usize,
}

/// Apply filter and compute summary statistics.
pub fn filter_with_summary(
    filter: &TestFilter,
    result: &TestRunResult,
) -> (TestRunResult, FilterSummary) {
    let total_before = result.total_tests();
    let suites_before = result.suites.len();

    let filtered = filter.apply(result);

    let total_after = filtered.total_tests();
    let suites_after = filtered.suites.len();

    let summary = FilterSummary {
        total_before,
        total_after,
        filtered_out: total_before.saturating_sub(total_after),
        suites_removed: suites_before.saturating_sub(suites_after),
    };

    (filtered, summary)
}

/// Extract a list of failed test names from a result for re-running.
pub fn failed_test_names(result: &TestRunResult) -> Vec<String> {
    let mut names = Vec::new();
    for suite in &result.suites {
        for test in &suite.tests {
            if test.status == TestStatus::Failed {
                names.push(test.name.clone());
            }
        }
    }
    names
}

/// Extract test names matching a filter.
pub fn matching_test_names(filter: &TestFilter, result: &TestRunResult) -> Vec<String> {
    let mut names = Vec::new();
    for suite in &result.suites {
        for test in &suite.tests {
            if filter.matches(test, &suite.name) {
                names.push(test.name.clone());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_test(name: &str, status: TestStatus) -> TestCase {
        TestCase {
            name: name.into(),
            status,
            duration: Duration::from_millis(1),
            error: None,
        }
    }

    fn make_suite(name: &str, tests: Vec<TestCase>) -> TestSuite {
        TestSuite {
            name: name.into(),
            tests,
        }
    }

    fn make_result(suites: Vec<TestSuite>) -> TestRunResult {
        TestRunResult {
            suites,
            duration: Duration::from_millis(100),
            raw_exit_code: 0,
        }
    }

    // ─── FilterPattern Tests ────────────────────────────────────────────

    #[test]
    fn pattern_exact() {
        let p = FilterPattern::parse("test_add");
        assert!(p.matches("test_add"));
        assert!(!p.matches("test_add_extra"));
        assert!(!p.matches("test"));
    }

    #[test]
    fn pattern_prefix() {
        let p = FilterPattern::parse("test_*");
        assert!(p.matches("test_add"));
        assert!(p.matches("test_"));
        assert!(!p.matches("other_add"));
    }

    #[test]
    fn pattern_suffix() {
        let p = FilterPattern::parse("*_add");
        assert!(p.matches("test_add"));
        assert!(p.matches("_add"));
        assert!(!p.matches("test_sub"));
    }

    #[test]
    fn pattern_contains() {
        let p = FilterPattern::parse("*divide*");
        assert!(p.matches("test_divide_by_zero"));
        assert!(p.matches("divide"));
        assert!(!p.matches("test_add"));
    }

    #[test]
    fn pattern_glob() {
        let p = FilterPattern::parse("test_*_basic_*");
        assert!(p.matches("test_math_basic_add"));
        assert!(p.matches("test_str_basic_concat"));
        assert!(!p.matches("test_math_advanced_add"));
    }

    #[test]
    fn pattern_exact_no_wildcard() {
        let p = FilterPattern::parse("hello");
        assert!(matches!(p, FilterPattern::Exact(_)));
    }

    // ─── TestFilter Tests ───────────────────────────────────────────────

    #[test]
    fn filter_include_single() {
        let filter = TestFilter::new().include("test_add");
        let test = make_test("test_add", TestStatus::Passed);
        assert!(filter.matches(&test, "suite"));

        let test2 = make_test("test_sub", TestStatus::Passed);
        assert!(!filter.matches(&test2, "suite"));
    }

    #[test]
    fn filter_include_csv() {
        let filter = TestFilter::new().include_csv("test_add, test_sub");
        assert!(filter.matches(&make_test("test_add", TestStatus::Passed), "s"));
        assert!(filter.matches(&make_test("test_sub", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_mul", TestStatus::Passed), "s"));
    }

    #[test]
    fn filter_exclude() {
        let filter = TestFilter::new().exclude("*slow*");
        assert!(filter.matches(&make_test("test_fast", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_slow_add", TestStatus::Passed), "s"));
    }

    #[test]
    fn filter_status() {
        let filter = TestFilter::new().status(TestStatus::Failed);
        assert!(!filter.matches(&make_test("test", TestStatus::Passed), "s"));
        assert!(filter.matches(&make_test("test", TestStatus::Failed), "s"));
    }

    #[test]
    fn filter_suite() {
        let filter = TestFilter::new().suite("MathTest");
        assert!(filter.matches(&make_test("test", TestStatus::Passed), "MathTest"));
        assert!(!filter.matches(&make_test("test", TestStatus::Passed), "StringTest"));
    }

    #[test]
    fn filter_combined() {
        let filter = TestFilter::new()
            .include("test_*")
            .exclude("*slow*")
            .status(TestStatus::Failed);

        assert!(!filter.matches(&make_test("test_add", TestStatus::Passed), "s")); // wrong status
        assert!(filter.matches(&make_test("test_add", TestStatus::Failed), "s")); // matches all
        assert!(!filter.matches(&make_test("test_slow", TestStatus::Failed), "s")); // excluded
        assert!(!filter.matches(&make_test("other", TestStatus::Failed), "s")); // no include match
    }

    #[test]
    fn filter_empty_matches_all() {
        let filter = TestFilter::new();
        assert!(!filter.is_active());
        assert!(filter.matches(&make_test("anything", TestStatus::Passed), "any"));
    }

    // ─── Apply Tests ────────────────────────────────────────────────────

    #[test]
    fn apply_filter_to_result() {
        let result = make_result(vec![make_suite(
            "tests",
            vec![
                make_test("test_add", TestStatus::Passed),
                make_test("test_sub", TestStatus::Passed),
                make_test("test_div", TestStatus::Failed),
            ],
        )]);

        let filter = TestFilter::new().status(TestStatus::Failed);
        let filtered = filter.apply(&result);

        assert_eq!(filtered.total_tests(), 1);
        assert_eq!(filtered.suites[0].tests[0].name, "test_div");
    }

    #[test]
    fn apply_filter_removes_empty_suites() {
        let result = make_result(vec![
            make_suite("MathTest", vec![make_test("test_add", TestStatus::Passed)]),
            make_suite(
                "StringTest",
                vec![make_test("test_upper", TestStatus::Passed)],
            ),
        ]);

        let filter = TestFilter::new().suite("MathTest");
        let filtered = filter.apply(&result);

        assert_eq!(filtered.suites.len(), 1);
        assert_eq!(filtered.suites[0].name, "MathTest");
    }

    #[test]
    fn apply_no_filter_returns_clone() {
        let result = make_result(vec![make_suite(
            "tests",
            vec![make_test("test_add", TestStatus::Passed)],
        )]);

        let filter = TestFilter::new();
        let filtered = filter.apply(&result);

        assert_eq!(filtered.total_tests(), result.total_tests());
    }

    // ─── Glob Matching Tests ────────────────────────────────────────────

    #[test]
    fn glob_segments_parsing() {
        let segs = parse_glob_segments("test_*_basic_*");
        assert_eq!(segs.len(), 4);
        assert!(matches!(&segs[0], GlobSegment::Literal(s) if s == "test_"));
        assert!(matches!(&segs[1], GlobSegment::Wildcard));
        assert!(matches!(&segs[2], GlobSegment::Literal(s) if s == "_basic_"));
        assert!(matches!(&segs[3], GlobSegment::Wildcard));
    }

    #[test]
    fn glob_match_basic() {
        let segs = parse_glob_segments("hello");
        assert!(glob_match(&segs, "hello"));
        assert!(!glob_match(&segs, "hell"));
    }

    #[test]
    fn glob_match_wildcard() {
        let segs = parse_glob_segments("*");
        assert!(glob_match(&segs, "anything"));
        assert!(glob_match(&segs, ""));
    }

    #[test]
    fn glob_match_complex() {
        let segs = parse_glob_segments("test_*_*_end");
        assert!(glob_match(&segs, "test_a_b_end"));
        assert!(glob_match(&segs, "test_foo_bar_end"));
        assert!(!glob_match(&segs, "test_end"));
    }

    // ─── Helper Function Tests ──────────────────────────────────────────

    #[test]
    fn build_filter_basic() {
        let filter = build_filter(Some("test_*"), Some("*slow*"), false);
        assert!(filter.is_active());
        assert!(filter.matches(&make_test("test_fast", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_slow", TestStatus::Passed), "s"));
    }

    #[test]
    fn build_filter_failed_only() {
        let filter = build_filter(None, None, true);
        assert!(filter.is_active());
        assert!(!filter.matches(&make_test("test", TestStatus::Passed), "s"));
        assert!(filter.matches(&make_test("test", TestStatus::Failed), "s"));
    }

    #[test]
    fn build_filter_none() {
        let filter = build_filter(None, None, false);
        assert!(!filter.is_active());
    }

    #[test]
    fn filter_with_summary_test() {
        let result = make_result(vec![make_suite(
            "tests",
            vec![
                make_test("test_a", TestStatus::Passed),
                make_test("test_b", TestStatus::Failed),
                make_test("test_c", TestStatus::Passed),
            ],
        )]);

        let filter = TestFilter::new().status(TestStatus::Failed);
        let (filtered, summary) = filter_with_summary(&filter, &result);

        assert_eq!(summary.total_before, 3);
        assert_eq!(summary.total_after, 1);
        assert_eq!(summary.filtered_out, 2);
        assert_eq!(filtered.total_failed(), 1);
    }

    #[test]
    fn failed_test_names_test() {
        let result = make_result(vec![make_suite(
            "tests",
            vec![
                make_test("test_a", TestStatus::Passed),
                make_test("test_b", TestStatus::Failed),
                make_test("test_c", TestStatus::Failed),
            ],
        )]);

        let names = failed_test_names(&result);
        assert_eq!(names, vec!["test_b", "test_c"]);
    }

    #[test]
    fn matching_test_names_test() {
        let result = make_result(vec![make_suite(
            "tests",
            vec![
                make_test("test_add", TestStatus::Passed),
                make_test("test_sub", TestStatus::Passed),
                make_test("other", TestStatus::Passed),
            ],
        )]);

        let filter = TestFilter::new().include("test_*");
        let names = matching_test_names(&filter, &result);
        assert_eq!(names, vec!["test_add", "test_sub"]);
    }

    #[test]
    fn exclude_csv_multiple() {
        let filter = TestFilter::new().exclude_csv("*slow*, *flaky*, *skip*");
        assert!(filter.matches(&make_test("test_fast", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_slow", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_flaky", TestStatus::Passed), "s"));
        assert!(!filter.matches(&make_test("test_skip_me", TestStatus::Passed), "s"));
    }
}
