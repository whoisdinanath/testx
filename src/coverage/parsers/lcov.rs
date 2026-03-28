//! LCOV coverage file parser.
//!
//! Parses LCOV trace files produced by gcov, llvm-cov, coverage.py, etc.
//! Format reference: https://manpages.debian.org/stretch/lcov/geninfo.1.en.html

use std::collections::HashMap;
use std::path::PathBuf;

use crate::coverage::{CoverageResult, FileCoverage};

/// Parse LCOV format coverage data.
///
/// LCOV format consists of records delimited by `end_of_record`.
/// Each record describes one source file with line and branch data.
///
/// Key records:
/// - `SF:path` — source file path
/// - `DA:line,count` — line hit data
/// - `BRDA:line,block,branch,count` — branch hit data
/// - `LF:count` — total lines found
/// - `LH:count` — total lines hit
/// - `BRF:count` — total branches found
/// - `BRH:count` — total branches hit
/// - `end_of_record` — end of file record
pub fn parse_lcov(content: &str) -> CoverageResult {
    let mut files = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut line_hits: HashMap<usize, u64> = HashMap::new();
    let mut total_branches: usize = 0;
    let mut covered_branches: usize = 0;

    for line in content.lines() {
        let line = line.trim();

        if let Some(path) = line.strip_prefix("SF:") {
            // Start new file record
            current_path = Some(PathBuf::from(path.trim()));
            line_hits.clear();
            total_branches = 0;
            covered_branches = 0;
        } else if let Some(da) = line.strip_prefix("DA:") {
            // Line data: DA:line_number,execution_count
            if let Some((line_str, count_str)) = da.split_once(',')
                && let (Ok(line_num), Ok(count)) = (
                    line_str.trim().parse::<usize>(),
                    count_str.trim().parse::<u64>(),
                )
            {
                let entry = line_hits.entry(line_num).or_insert(0);
                *entry = (*entry).max(count);
            }
        } else if let Some(brda) = line.strip_prefix("BRDA:") {
            // Branch data: BRDA:line,block,branch,taken
            let parts: Vec<&str> = brda.splitn(4, ',').collect();
            if parts.len() == 4 {
                total_branches += 1;
                if parts[3].trim() != "-" && parts[3].trim() != "0" {
                    covered_branches += 1;
                }
            }
        } else if line == "end_of_record" {
            // Finalize current file
            if let Some(path) = current_path.take() {
                let total_lines = line_hits.len();
                let covered_lines = line_hits.values().filter(|&&c| c > 0).count();

                files.push(FileCoverage {
                    path,
                    total_lines,
                    covered_lines,
                    uncovered_ranges: Vec::new(),
                    line_hits: line_hits.clone(),
                    total_branches,
                    covered_branches,
                });
            }
            line_hits.clear();
            total_branches = 0;
            covered_branches = 0;
        }
        // Ignore other records (FN, FNDA, FNF, FNH, etc.)
    }

    // Handle case where file doesn't end with end_of_record
    if let Some(path) = current_path.take() {
        let total_lines = line_hits.len();
        let covered_lines = line_hits.values().filter(|&&c| c > 0).count();
        files.push(FileCoverage {
            path,
            total_lines,
            covered_lines,
            uncovered_ranges: Vec::new(),
            line_hits,
            total_branches,
            covered_branches,
        });
    }

    CoverageResult::from_files(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_file() {
        let lcov = "\
SF:src/main.rs
DA:1,5
DA:2,3
DA:3,0
DA:4,1
LF:4
LH:3
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.total_lines, 4);
        assert_eq!(result.covered_lines, 3);
        assert!((result.percentage - 75.0).abs() < 0.1);
    }

    #[test]
    fn parse_multiple_files() {
        let lcov = "\
SF:src/a.rs
DA:1,1
DA:2,1
end_of_record
SF:src/b.rs
DA:1,0
DA:2,0
DA:3,1
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.total_lines, 5);
        assert_eq!(result.covered_lines, 3);
    }

    #[test]
    fn parse_with_branches() {
        let lcov = "\
SF:src/main.rs
DA:1,1
BRDA:1,0,0,1
BRDA:1,0,1,0
BRF:2
BRH:1
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.total_branches, 2);
        assert_eq!(result.covered_branches, 1);
    }

    #[test]
    fn parse_empty() {
        let result = parse_lcov("");
        assert_eq!(result.files.len(), 0);
        assert_eq!(result.percentage, 0.0);
    }

    #[test]
    fn parse_no_end_of_record() {
        let lcov = "\
SF:src/main.rs
DA:1,1
DA:2,0
";
        let result = parse_lcov(lcov);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.covered_lines, 1);
    }

    #[test]
    fn parse_branch_dash() {
        let lcov = "\
SF:src/main.rs
BRDA:1,0,0,-
BRDA:1,0,1,5
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.total_branches, 2);
        assert_eq!(result.covered_branches, 1);
    }

    #[test]
    fn parse_line_hits_max() {
        // Multiple DA entries for same line should take max
        let lcov = "\
SF:src/main.rs
DA:1,3
DA:1,5
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.files[0].line_hits[&1], 5);
    }

    #[test]
    fn parse_file_path_preserved() {
        let lcov = "\
SF:/home/user/project/src/lib.rs
DA:1,1
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(
            result.files[0].path,
            PathBuf::from("/home/user/project/src/lib.rs")
        );
    }

    #[test]
    fn parse_ignores_unknown_records() {
        let lcov = "\
TN:test_name
SF:src/main.rs
FN:1,main
FNDA:5,main
FNF:1
FNH:1
DA:1,5
LF:1
LH:1
end_of_record
";
        let result = parse_lcov(lcov);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.covered_lines, 1);
    }

    #[test]
    fn parse_whitespace_handling() {
        let lcov = "  SF:src/main.rs  \n  DA:1,1  \n  end_of_record  \n";
        let result = parse_lcov(lcov);
        assert_eq!(result.files.len(), 1);
    }
}
