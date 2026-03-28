//! Coverage integration module.
//!
//! Provides a unified interface for collecting and displaying
//! code coverage across all supported languages/adapters.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod display;
pub mod parsers;

/// Coverage configuration.
#[derive(Debug, Clone)]
pub struct CoverageConfig {
    /// Whether coverage collection is enabled
    pub enabled: bool,
    /// Output format for coverage data
    pub format: CoverageFormat,
    /// Directory for coverage output files
    pub output_dir: PathBuf,
    /// Minimum coverage threshold (fail if below)
    pub threshold: Option<f64>,
    /// Paths to include in coverage (glob patterns)
    pub include: Vec<String>,
    /// Paths to exclude from coverage (glob patterns)
    pub exclude: Vec<String>,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: CoverageFormat::Summary,
            output_dir: PathBuf::from("coverage"),
            threshold: None,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

/// Output format for coverage reports.
#[derive(Debug, Clone, PartialEq)]
pub enum CoverageFormat {
    /// Text summary only
    Summary,
    /// LCOV format
    Lcov,
    /// Cobertura XML
    Cobertura,
    /// HTML report
    Html,
    /// JSON data
    Json,
}

impl CoverageFormat {
    /// Parse a format string (case-insensitive).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "lcov" => CoverageFormat::Lcov,
            "cobertura" | "xml" => CoverageFormat::Cobertura,
            "html" => CoverageFormat::Html,
            "json" => CoverageFormat::Json,
            _ => CoverageFormat::Summary,
        }
    }

    /// File extension for this format.
    pub fn extension(&self) -> &str {
        match self {
            CoverageFormat::Summary => "txt",
            CoverageFormat::Lcov => "lcov",
            CoverageFormat::Cobertura => "xml",
            CoverageFormat::Html => "html",
            CoverageFormat::Json => "json",
        }
    }
}

/// Complete coverage result for a project.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CoverageResult {
    /// Per-file coverage data
    pub files: Vec<FileCoverage>,
    /// Total lines in all files
    pub total_lines: usize,
    /// Total covered lines
    pub covered_lines: usize,
    /// Overall coverage percentage
    pub percentage: f64,
    /// Total branches (if available)
    pub total_branches: usize,
    /// Covered branches (if available)
    pub covered_branches: usize,
    /// Branch coverage percentage
    pub branch_percentage: f64,
}

impl CoverageResult {
    /// Create a CoverageResult from a vector of file coverage data.
    pub fn from_files(files: Vec<FileCoverage>) -> Self {
        let total_lines: usize = files.iter().map(|f| f.total_lines).sum();
        let covered_lines: usize = files.iter().map(|f| f.covered_lines).sum();
        let total_branches: usize = files.iter().map(|f| f.total_branches).sum();
        let covered_branches: usize = files.iter().map(|f| f.covered_branches).sum();

        let percentage = if total_lines > 0 {
            covered_lines as f64 / total_lines as f64 * 100.0
        } else {
            0.0
        };

        let branch_percentage = if total_branches > 0 {
            covered_branches as f64 / total_branches as f64 * 100.0
        } else {
            0.0
        };

        Self {
            files,
            total_lines,
            covered_lines,
            percentage,
            total_branches,
            covered_branches,
            branch_percentage,
        }
    }

    /// Check if coverage meets a minimum threshold.
    pub fn meets_threshold(&self, threshold: f64) -> bool {
        self.percentage >= threshold
    }

    /// Get files sorted by coverage percentage (lowest first).
    pub fn worst_files(&self, n: usize) -> Vec<&FileCoverage> {
        let mut sorted: Vec<&FileCoverage> = self.files.iter().collect();
        sorted.sort_by(|a, b| {
            a.percentage()
                .partial_cmp(&b.percentage())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.into_iter().take(n).collect()
    }

    /// Get the number of uncovered files.
    pub fn uncovered_file_count(&self) -> usize {
        self.files.iter().filter(|f| f.covered_lines == 0).count()
    }

    /// Filter files by a predicate.
    pub fn filter_files<F>(&self, predicate: F) -> Self
    where
        F: Fn(&FileCoverage) -> bool,
    {
        let files: Vec<FileCoverage> = self
            .files
            .iter()
            .filter(|f| predicate(f))
            .cloned()
            .collect();
        Self::from_files(files)
    }
}

/// Coverage data for a single file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileCoverage {
    /// Relative path to the file
    pub path: PathBuf,
    /// Total executable lines
    pub total_lines: usize,
    /// Number of lines with coverage
    pub covered_lines: usize,
    /// Uncovered line ranges: [(start, end), ...]
    pub uncovered_ranges: Vec<(usize, usize)>,
    /// Per-line hit counts: line_number -> hit_count
    #[serde(skip)]
    pub line_hits: HashMap<usize, u64>,
    /// Total branches in the file
    pub total_branches: usize,
    /// Covered branches
    pub covered_branches: usize,
}

impl FileCoverage {
    /// Coverage percentage for this file.
    pub fn percentage(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            self.covered_lines as f64 / self.total_lines as f64 * 100.0
        }
    }

    /// Branch coverage percentage for this file.
    pub fn branch_percentage(&self) -> f64 {
        if self.total_branches == 0 {
            0.0
        } else {
            self.covered_branches as f64 / self.total_branches as f64 * 100.0
        }
    }

    /// Whether this file has full line coverage.
    pub fn is_fully_covered(&self) -> bool {
        self.covered_lines == self.total_lines && self.total_lines > 0
    }
}

/// Trait for language-specific coverage providers.
pub trait CoverageProvider {
    /// Return extra CLI arguments to enable coverage for this adapter.
    fn coverage_args(&self) -> Vec<String>;

    /// Parse coverage data from the output directory.
    fn parse_coverage(&self, output_dir: &Path) -> crate::error::Result<CoverageResult>;

    /// Name of the coverage tool being used.
    fn tool_name(&self) -> &str;
}

/// Adapter-specific coverage configurations.
#[derive(Debug, Clone)]
pub struct AdapterCoverageConfig {
    /// Adapter name
    pub adapter: String,
    /// Coverage tool to use
    pub tool: String,
    /// Extra arguments for coverage collection
    pub extra_args: Vec<String>,
    /// Environment variables for coverage
    pub env: HashMap<String, String>,
}

/// Known coverage tools per adapter.
pub fn default_coverage_tool(adapter: &str) -> Option<AdapterCoverageConfig> {
    let config = match adapter {
        "rust" => AdapterCoverageConfig {
            adapter: "rust".into(),
            tool: "cargo-llvm-cov".into(),
            extra_args: vec!["--lcov".into(), "--output-path".into()],
            env: HashMap::new(),
        },
        "python" => AdapterCoverageConfig {
            adapter: "python".into(),
            tool: "coverage".into(),
            extra_args: vec!["run".into(), "-m".into(), "pytest".into()],
            env: HashMap::new(),
        },
        "javascript" => AdapterCoverageConfig {
            adapter: "javascript".into(),
            tool: "built-in".into(),
            extra_args: vec!["--coverage".into()],
            env: HashMap::new(),
        },
        "go" => AdapterCoverageConfig {
            adapter: "go".into(),
            tool: "go-cover".into(),
            extra_args: vec!["-coverprofile=coverage.out".into()],
            env: HashMap::new(),
        },
        "java" => AdapterCoverageConfig {
            adapter: "java".into(),
            tool: "jacoco".into(),
            extra_args: Vec::new(),
            env: HashMap::new(),
        },
        "cpp" => AdapterCoverageConfig {
            adapter: "cpp".into(),
            tool: "gcov".into(),
            extra_args: vec!["--coverage".into()],
            env: HashMap::new(),
        },
        "ruby" => AdapterCoverageConfig {
            adapter: "ruby".into(),
            tool: "simplecov".into(),
            extra_args: Vec::new(),
            env: HashMap::from([("COVERAGE".into(), "true".into())]),
        },
        "elixir" => AdapterCoverageConfig {
            adapter: "elixir".into(),
            tool: "mix-cover".into(),
            extra_args: vec!["--cover".into()],
            env: HashMap::new(),
        },
        "dotnet" => AdapterCoverageConfig {
            adapter: "dotnet".into(),
            tool: "xplat-coverage".into(),
            extra_args: vec!["--collect:\"XPlat Code Coverage\"".into()],
            env: HashMap::new(),
        },
        _ => return None,
    };
    Some(config)
}

/// Merge multiple coverage results (e.g. from parallel adapter runs).
pub fn merge_coverage(results: &[CoverageResult]) -> CoverageResult {
    let mut file_map: HashMap<PathBuf, FileCoverage> = HashMap::new();

    for result in results {
        for file in &result.files {
            let entry = file_map
                .entry(file.path.clone())
                .or_insert_with(|| FileCoverage {
                    path: file.path.clone(),
                    total_lines: 0,
                    covered_lines: 0,
                    uncovered_ranges: Vec::new(),
                    line_hits: HashMap::new(),
                    total_branches: 0,
                    covered_branches: 0,
                });

            // Merge line hits (take max)
            for (&line, &hits) in &file.line_hits {
                let existing = entry.line_hits.entry(line).or_insert(0);
                *existing = (*existing).max(hits);
            }

            // Recalculate from merged line_hits
            entry.total_lines = entry.total_lines.max(file.total_lines);
            entry.covered_lines = entry.line_hits.values().filter(|&&h| h > 0).count();
            entry.total_branches = entry.total_branches.max(file.total_branches);
            entry.covered_branches = entry.covered_branches.max(file.covered_branches);
        }
    }

    // Recompute uncovered ranges from merged line hits
    let files: Vec<FileCoverage> = file_map
        .into_values()
        .map(|mut f| {
            f.uncovered_ranges = compute_uncovered_ranges(&f.line_hits, f.total_lines);
            f
        })
        .collect();

    CoverageResult::from_files(files)
}

/// Compute contiguous uncovered line ranges from per-line hit data.
fn compute_uncovered_ranges(
    line_hits: &HashMap<usize, u64>,
    total_lines: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;

    for line in 1..=total_lines {
        let is_covered = line_hits.get(&line).is_some_and(|&h| h > 0);
        let is_executable = line_hits.contains_key(&line);

        if is_executable && !is_covered {
            if start.is_none() {
                start = Some(line);
            }
        } else if let Some(s) = start {
            ranges.push((s, line - 1));
            start = None;
        }
    }

    if let Some(s) = start {
        ranges.push((s, total_lines));
    }

    ranges
}

/// Compute a coverage delta between two results.
pub fn coverage_delta(old: &CoverageResult, new: &CoverageResult) -> CoverageDelta {
    let line_delta = new.percentage - old.percentage;
    let branch_delta = new.branch_percentage - old.branch_percentage;

    let mut file_deltas = Vec::new();
    let old_map: HashMap<&Path, &FileCoverage> =
        old.files.iter().map(|f| (f.path.as_path(), f)).collect();

    for file in &new.files {
        if let Some(old_file) = old_map.get(file.path.as_path()) {
            let delta = file.percentage() - old_file.percentage();
            if delta.abs() > 0.01 {
                file_deltas.push(FileCoverageDelta {
                    path: file.path.clone(),
                    old_percentage: old_file.percentage(),
                    new_percentage: file.percentage(),
                    delta,
                });
            }
        } else {
            file_deltas.push(FileCoverageDelta {
                path: file.path.clone(),
                old_percentage: 0.0,
                new_percentage: file.percentage(),
                delta: file.percentage(),
            });
        }
    }

    // Sort by absolute delta, largest first
    file_deltas.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    CoverageDelta {
        line_delta,
        branch_delta,
        file_deltas,
    }
}

/// Overall coverage change between two runs.
#[derive(Debug, Clone)]
pub struct CoverageDelta {
    /// Change in line coverage percentage
    pub line_delta: f64,
    /// Change in branch coverage percentage
    pub branch_delta: f64,
    /// Per-file coverage changes
    pub file_deltas: Vec<FileCoverageDelta>,
}

impl CoverageDelta {
    /// Whether coverage improved.
    pub fn improved(&self) -> bool {
        self.line_delta > 0.0
    }

    /// Whether coverage regressed.
    pub fn regressed(&self) -> bool {
        self.line_delta < -0.01
    }

    /// Format delta as a string with direction indicator.
    pub fn format_delta(&self) -> String {
        let arrow = if self.line_delta > 0.0 {
            "↑"
        } else if self.line_delta < -0.01 {
            "↓"
        } else {
            "→"
        };
        format!("{arrow} {:.1}%", self.line_delta.abs())
    }
}

/// Coverage change for a single file.
#[derive(Debug, Clone)]
pub struct FileCoverageDelta {
    pub path: PathBuf,
    pub old_percentage: f64,
    pub new_percentage: f64,
    pub delta: f64,
}

/// Check if a file should be included in coverage based on include/exclude patterns.
pub fn should_include_file(path: &Path, include: &[String], exclude: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    // If includes are specified, file must match at least one
    if !include.is_empty() {
        let matches_include = include.iter().any(|pattern| glob_match(pattern, &path_str));
        if !matches_include {
            return false;
        }
    }

    // File must not match any exclude pattern
    !exclude.iter().any(|pattern| glob_match(pattern, &path_str))
}

/// Simple glob matching for coverage include/exclude patterns.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = text[pos..].find(part) {
            if i == 0 && found != 0 {
                return false; // Must start with first part
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }

    // If pattern doesn't end with *, text must end at pos
    if !pattern.ends_with('*') && pos != text.len() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(path: &str, total: usize, covered: usize) -> FileCoverage {
        let mut line_hits = HashMap::new();
        for i in 1..=total {
            line_hits.insert(i, if i <= covered { 1 } else { 0 });
        }
        FileCoverage {
            path: PathBuf::from(path),
            total_lines: total,
            covered_lines: covered,
            uncovered_ranges: Vec::new(),
            line_hits,
            total_branches: 0,
            covered_branches: 0,
        }
    }

    #[test]
    fn coverage_from_files() {
        let result =
            CoverageResult::from_files(vec![make_file("a.rs", 100, 80), make_file("b.rs", 50, 50)]);
        assert_eq!(result.total_lines, 150);
        assert_eq!(result.covered_lines, 130);
        assert!((result.percentage - 86.66).abs() < 0.1);
    }

    #[test]
    fn coverage_empty() {
        let result = CoverageResult::from_files(vec![]);
        assert_eq!(result.total_lines, 0);
        assert_eq!(result.percentage, 0.0);
    }

    #[test]
    fn coverage_meets_threshold() {
        let result = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        assert!(result.meets_threshold(80.0));
        assert!(!result.meets_threshold(81.0));
    }

    #[test]
    fn coverage_worst_files() {
        let result = CoverageResult::from_files(vec![
            make_file("good.rs", 100, 95),
            make_file("bad.rs", 100, 20),
            make_file("ok.rs", 100, 60),
        ]);
        let worst = result.worst_files(2);
        assert_eq!(worst.len(), 2);
        assert_eq!(worst[0].path, PathBuf::from("bad.rs"));
        assert_eq!(worst[1].path, PathBuf::from("ok.rs"));
    }

    #[test]
    fn coverage_uncovered_count() {
        let result = CoverageResult::from_files(vec![
            make_file("a.rs", 100, 0),
            make_file("b.rs", 50, 50),
            make_file("c.rs", 75, 0),
        ]);
        assert_eq!(result.uncovered_file_count(), 2);
    }

    #[test]
    fn file_percentage() {
        let file = make_file("a.rs", 100, 75);
        assert_eq!(file.percentage(), 75.0);
    }

    #[test]
    fn file_percentage_zero() {
        let file = make_file("a.rs", 0, 0);
        assert_eq!(file.percentage(), 0.0);
    }

    #[test]
    fn file_fully_covered() {
        let full = make_file("full.rs", 50, 50);
        let partial = make_file("partial.rs", 50, 40);
        let empty = make_file("empty.rs", 0, 0);
        assert!(full.is_fully_covered());
        assert!(!partial.is_fully_covered());
        assert!(!empty.is_fully_covered());
    }

    #[test]
    fn format_from_str() {
        assert_eq!(CoverageFormat::from_str_lossy("lcov"), CoverageFormat::Lcov);
        assert_eq!(
            CoverageFormat::from_str_lossy("cobertura"),
            CoverageFormat::Cobertura
        );
        assert_eq!(
            CoverageFormat::from_str_lossy("XML"),
            CoverageFormat::Cobertura
        );
        assert_eq!(CoverageFormat::from_str_lossy("html"), CoverageFormat::Html);
        assert_eq!(CoverageFormat::from_str_lossy("json"), CoverageFormat::Json);
        assert_eq!(
            CoverageFormat::from_str_lossy("unknown"),
            CoverageFormat::Summary
        );
    }

    #[test]
    fn format_extension() {
        assert_eq!(CoverageFormat::Summary.extension(), "txt");
        assert_eq!(CoverageFormat::Lcov.extension(), "lcov");
        assert_eq!(CoverageFormat::Cobertura.extension(), "xml");
    }

    #[test]
    fn default_coverage_tools() {
        assert!(default_coverage_tool("rust").is_some());
        assert!(default_coverage_tool("python").is_some());
        assert!(default_coverage_tool("javascript").is_some());
        assert!(default_coverage_tool("go").is_some());
        assert!(default_coverage_tool("java").is_some());
        assert!(default_coverage_tool("cpp").is_some());
        assert!(default_coverage_tool("ruby").is_some());
        assert!(default_coverage_tool("elixir").is_some());
        assert!(default_coverage_tool("dotnet").is_some());
        assert!(default_coverage_tool("unknown").is_none());
    }

    #[test]
    fn coverage_delta_improved() {
        let old = CoverageResult::from_files(vec![make_file("a.rs", 100, 70)]);
        let new = CoverageResult::from_files(vec![make_file("a.rs", 100, 85)]);
        let delta = coverage_delta(&old, &new);
        assert!(delta.improved());
        assert!(!delta.regressed());
        assert!(delta.format_delta().contains("↑"));
    }

    #[test]
    fn coverage_delta_regressed() {
        let old = CoverageResult::from_files(vec![make_file("a.rs", 100, 85)]);
        let new = CoverageResult::from_files(vec![make_file("a.rs", 100, 70)]);
        let delta = coverage_delta(&old, &new);
        assert!(delta.regressed());
        assert!(!delta.improved());
        assert!(delta.format_delta().contains("↓"));
    }

    #[test]
    fn coverage_delta_stable() {
        let old = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        let new = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        let delta = coverage_delta(&old, &new);
        assert!(!delta.improved());
        assert!(!delta.regressed());
    }

    #[test]
    fn coverage_delta_new_file() {
        let old = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        let new =
            CoverageResult::from_files(vec![make_file("a.rs", 100, 80), make_file("b.rs", 50, 40)]);
        let delta = coverage_delta(&old, &new);
        let new_file = delta
            .file_deltas
            .iter()
            .find(|d| d.path == Path::new("b.rs"));
        assert!(new_file.is_some());
        assert_eq!(new_file.unwrap().old_percentage, 0.0);
    }

    #[test]
    fn merge_two_results() {
        let r1 = CoverageResult::from_files(vec![make_file("a.rs", 100, 50)]);
        let r2 = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        let merged = merge_coverage(&[r1, r2]);
        assert_eq!(merged.files.len(), 1);
        // Merged should take max hits, so covered >= 80
        assert!(merged.covered_lines >= 80);
    }

    #[test]
    fn merge_different_files() {
        let r1 = CoverageResult::from_files(vec![make_file("a.rs", 100, 50)]);
        let r2 = CoverageResult::from_files(vec![make_file("b.rs", 50, 40)]);
        let merged = merge_coverage(&[r1, r2]);
        assert_eq!(merged.files.len(), 2);
    }

    #[test]
    fn uncovered_ranges() {
        let mut hits = HashMap::new();
        hits.insert(1, 5); // covered
        hits.insert(2, 0); // uncovered
        hits.insert(3, 0); // uncovered
        hits.insert(4, 3); // covered
        hits.insert(5, 0); // uncovered

        let ranges = compute_uncovered_ranges(&hits, 5);
        assert_eq!(ranges, vec![(2, 3), (5, 5)]);
    }

    #[test]
    fn uncovered_ranges_all_covered() {
        let mut hits = HashMap::new();
        hits.insert(1, 1);
        hits.insert(2, 1);
        hits.insert(3, 1);
        let ranges = compute_uncovered_ranges(&hits, 3);
        assert!(ranges.is_empty());
    }

    #[test]
    fn glob_match_simple() {
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("*.rs", "foo.py"));
    }

    #[test]
    fn glob_match_double_star() {
        assert!(glob_match("src/*", "src/foo/bar.rs"));
    }

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("main.rs", "main.rs"));
        assert!(!glob_match("main.rs", "src/main.rs"));
    }

    #[test]
    fn should_include_defaults() {
        let path = Path::new("src/main.rs");
        assert!(should_include_file(path, &[], &[]));
    }

    #[test]
    fn should_include_with_include() {
        let path = Path::new("src/main.rs");
        assert!(should_include_file(path, &["src/*".into()], &[]));
        assert!(!should_include_file(path, &["tests/*".into()], &[]));
    }

    #[test]
    fn should_include_with_exclude() {
        let path = Path::new("src/vendor/lib.rs");
        assert!(!should_include_file(path, &[], &["*vendor*".into()]));
        assert!(should_include_file(path, &[], &["*test*".into()]));
    }

    #[test]
    fn filter_files_predicate() {
        let result = CoverageResult::from_files(vec![
            make_file("src/main.rs", 100, 80),
            make_file("tests/test.rs", 50, 50),
            make_file("src/lib.rs", 200, 150),
        ]);
        let filtered = result.filter_files(|f| f.path.starts_with("src"));
        assert_eq!(filtered.files.len(), 2);
        assert_eq!(filtered.total_lines, 300);
    }

    #[test]
    fn config_default() {
        let config = CoverageConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.format, CoverageFormat::Summary);
        assert!(config.threshold.is_none());
    }

    #[test]
    fn branch_coverage() {
        let file = FileCoverage {
            path: PathBuf::from("a.rs"),
            total_lines: 100,
            covered_lines: 80,
            uncovered_ranges: Vec::new(),
            line_hits: HashMap::new(),
            total_branches: 20,
            covered_branches: 15,
        };
        assert_eq!(file.branch_percentage(), 75.0);
    }
}
