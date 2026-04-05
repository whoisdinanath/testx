//! Coverage display and formatting.
//!
//! Pretty-prints coverage results, highlights uncovered files,
//! and shows delta vs previous run.

use std::fmt::Write;

use crate::coverage::{CoverageDelta, CoverageResult, FileCoverage};

/// Format a full coverage summary for terminal output.
pub fn format_coverage_summary(result: &CoverageResult) -> String {
    let mut out = String::with_capacity(2048);

    write_header(&mut out, result);
    write_file_table(&mut out, result);

    if result.total_branches > 0 {
        write_branch_summary(&mut out, result);
    }

    if result.uncovered_file_count() > 0 {
        write_uncovered_files(&mut out, result);
    }

    out
}

fn write_header(out: &mut String, result: &CoverageResult) {
    let _ = writeln!(out);
    let _ = writeln!(out, "  Coverage Summary");
    let _ = writeln!(out, "  ═══════════════════════════════════════");
    let _ = writeln!(
        out,
        "  Lines:    {}/{} ({:.1}%)",
        result.covered_lines, result.total_lines, result.percentage
    );
    if result.total_branches > 0 {
        let _ = writeln!(
            out,
            "  Branches: {}/{} ({:.1}%)",
            result.covered_branches, result.total_branches, result.branch_percentage
        );
    }
    let _ = writeln!(out, "  Files:    {}", result.files.len());
    let _ = writeln!(out);
}

fn write_file_table(out: &mut String, result: &CoverageResult) {
    if result.files.is_empty() {
        return;
    }

    // Find max filename length for alignment
    let max_name = result
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().len())
        .max()
        .unwrap_or(10)
        .min(60);

    let _ = writeln!(
        out,
        "  {:<width$}  {:>6}  {:>6}  {:>7}",
        "File",
        "Lines",
        "Cover",
        "Pct",
        width = max_name
    );
    let _ = writeln!(
        out,
        "  {:<width$}  {:>6}  {:>6}  {:>7}",
        "─".repeat(max_name),
        "──────",
        "──────",
        "───────",
        width = max_name
    );

    let mut sorted_files: Vec<&FileCoverage> = result.files.iter().collect();
    sorted_files.sort_by(|a, b| {
        a.percentage()
            .partial_cmp(&b.percentage())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for file in &sorted_files {
        let name = file.path.to_string_lossy();
        let display_name = if name.len() > max_name && max_name > 0 {
            let start = name.ceil_char_boundary(name.len().saturating_sub(max_name - 1));
            format!("…{}", &name[start..])
        } else {
            name.to_string()
        };

        let bar = coverage_bar(file.percentage(), 7);
        let _ = writeln!(
            out,
            "  {:<width$}  {:>6}  {:>6}  {} {:.1}%",
            display_name,
            file.total_lines,
            file.covered_lines,
            bar,
            file.percentage(),
            width = max_name
        );
    }
    let _ = writeln!(out);
}

fn write_branch_summary(out: &mut String, result: &CoverageResult) {
    let _ = writeln!(
        out,
        "  Branch Coverage: {}/{} ({:.1}%)",
        result.covered_branches, result.total_branches, result.branch_percentage
    );
    let _ = writeln!(out);
}

fn write_uncovered_files(out: &mut String, result: &CoverageResult) {
    let uncovered: Vec<&FileCoverage> = result
        .files
        .iter()
        .filter(|f| f.covered_lines == 0 && f.total_lines > 0)
        .collect();

    if uncovered.is_empty() {
        return;
    }

    let _ = writeln!(out, "  Uncovered Files ({}):", uncovered.len());
    for file in &uncovered {
        let _ = writeln!(
            out,
            "    ⚠ {} ({} lines)",
            file.path.display(),
            file.total_lines
        );
    }
    let _ = writeln!(out);
}

/// Generate an ASCII coverage bar.
fn coverage_bar(percentage: f64, width: usize) -> String {
    let filled = ((percentage / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    format!("│{}{}│", "█".repeat(filled), "░".repeat(empty))
}

/// Format a threshold check result.
pub fn format_threshold_check(result: &CoverageResult, threshold: f64) -> String {
    let met = result.meets_threshold(threshold);
    if met {
        format!(
            "  ✅ Coverage {:.1}% meets threshold {:.1}%",
            result.percentage, threshold
        )
    } else {
        format!(
            "  ❌ Coverage {:.1}% is below threshold {:.1}% (need {:.1}% more)",
            result.percentage,
            threshold,
            threshold - result.percentage
        )
    }
}

/// Format a coverage delta for display.
pub fn format_coverage_delta(delta: &CoverageDelta) -> String {
    let mut out = String::with_capacity(512);

    let _ = writeln!(out, "  Coverage Change: {}", delta.format_delta());
    let _ = writeln!(out);

    if !delta.file_deltas.is_empty() {
        let _ = writeln!(out, "  Changed Files:");
        let count = delta.file_deltas.len().min(10);
        for fd in delta.file_deltas.iter().take(count) {
            let arrow = if fd.delta > 0.0 { "↑" } else { "↓" };
            let _ = writeln!(
                out,
                "    {} {} {:.1}% → {:.1}% ({}{:.1}%)",
                arrow,
                fd.path.display(),
                fd.old_percentage,
                fd.new_percentage,
                if fd.delta > 0.0 { "+" } else { "" },
                fd.delta,
            );
        }
        if delta.file_deltas.len() > count {
            let _ = writeln!(
                out,
                "    ... and {} more files",
                delta.file_deltas.len() - count
            );
        }
    }

    out
}

/// Format coverage result as JSON string.
pub fn format_coverage_json(result: &CoverageResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::FileCoverageDelta;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_file(path: &str, total: usize, covered: usize) -> FileCoverage {
        FileCoverage {
            path: PathBuf::from(path),
            total_lines: total,
            covered_lines: covered,
            uncovered_ranges: Vec::new(),
            line_hits: HashMap::new(),
            total_branches: 0,
            covered_branches: 0,
        }
    }

    fn make_result() -> CoverageResult {
        CoverageResult::from_files(vec![
            make_file("src/main.rs", 100, 80),
            make_file("src/lib.rs", 200, 190),
            make_file("src/util.rs", 50, 0),
        ])
    }

    #[test]
    fn summary_contains_header() {
        let summary = format_coverage_summary(&make_result());
        assert!(summary.contains("Coverage Summary"));
        assert!(summary.contains("Lines:"));
    }

    #[test]
    fn summary_contains_totals() {
        let summary = format_coverage_summary(&make_result());
        assert!(summary.contains("270")); // covered
        assert!(summary.contains("350")); // total
    }

    #[test]
    fn summary_contains_files() {
        let summary = format_coverage_summary(&make_result());
        assert!(summary.contains("src/main.rs"));
        assert!(summary.contains("src/lib.rs"));
        assert!(summary.contains("src/util.rs"));
    }

    #[test]
    fn summary_uncovered_files() {
        let summary = format_coverage_summary(&make_result());
        assert!(summary.contains("Uncovered Files"));
        assert!(summary.contains("src/util.rs"));
    }

    #[test]
    fn coverage_bar_full() {
        let bar = coverage_bar(100.0, 5);
        assert!(bar.contains("█████"));
    }

    #[test]
    fn coverage_bar_empty() {
        let bar = coverage_bar(0.0, 5);
        assert!(bar.contains("░░░░░"));
    }

    #[test]
    fn coverage_bar_half() {
        let bar = coverage_bar(50.0, 4);
        assert!(bar.contains("██"));
        assert!(bar.contains("░░"));
    }

    #[test]
    fn threshold_met() {
        let result = CoverageResult::from_files(vec![make_file("a.rs", 100, 85)]);
        let msg = format_threshold_check(&result, 80.0);
        assert!(msg.contains("✅"));
        assert!(msg.contains("meets"));
    }

    #[test]
    fn threshold_not_met() {
        let result = CoverageResult::from_files(vec![make_file("a.rs", 100, 70)]);
        let msg = format_threshold_check(&result, 80.0);
        assert!(msg.contains("❌"));
        assert!(msg.contains("below"));
    }

    #[test]
    fn delta_format() {
        let delta = CoverageDelta {
            line_delta: 5.0,
            branch_delta: 0.0,
            file_deltas: vec![FileCoverageDelta {
                path: PathBuf::from("a.rs"),
                old_percentage: 70.0,
                new_percentage: 75.0,
                delta: 5.0,
            }],
        };
        let formatted = format_coverage_delta(&delta);
        assert!(formatted.contains("↑"));
        assert!(formatted.contains("a.rs"));
    }

    #[test]
    fn coverage_json() {
        let result = CoverageResult::from_files(vec![make_file("a.rs", 100, 80)]);
        let json = format_coverage_json(&result);
        assert!(json.contains("percentage"));
        assert!(json.contains("80"));
    }

    #[test]
    fn empty_result_summary() {
        let result = CoverageResult::from_files(vec![]);
        let summary = format_coverage_summary(&result);
        assert!(summary.contains("Coverage Summary"));
        assert!(summary.contains("0/0"));
    }

    #[test]
    fn branch_coverage_in_summary() {
        let result = CoverageResult {
            files: vec![],
            total_lines: 100,
            covered_lines: 80,
            percentage: 80.0,
            total_branches: 20,
            covered_branches: 15,
            branch_percentage: 75.0,
        };
        let summary = format_coverage_summary(&result);
        assert!(summary.contains("Branch Coverage"));
    }
}
