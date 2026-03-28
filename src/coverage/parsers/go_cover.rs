//! Go coverage profile parser.
//!
//! Parses Go's `-coverprofile` output format.
//!
//! Format: `<filename>:<startline>.<startcol>,<endline>.<endcol> <numstmt> <count>`
//! Example: `github.com/user/pkg/main.go:10.14,12.2 1 5`

use std::collections::HashMap;
use std::path::PathBuf;

use crate::coverage::{CoverageResult, FileCoverage};

/// Parse Go coverage profile data.
///
/// The first line is a mode declaration: `mode: set|count|atomic`
/// Subsequent lines describe covered blocks.
pub fn parse_go_cover(content: &str) -> CoverageResult {
    let mut file_map: HashMap<String, FileBuilder> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip mode line and empty lines
        if line.is_empty() || line.starts_with("mode:") {
            continue;
        }

        if let Some(block) = parse_coverage_line(line) {
            let builder = file_map
                .entry(block.file.clone())
                .or_insert_with(|| FileBuilder {
                    path: block.file.clone(),
                    line_hits: HashMap::new(),
                });

            // Mark all lines in the block
            for line_num in block.start_line..=block.end_line {
                let entry = builder.line_hits.entry(line_num).or_insert(0);
                *entry = (*entry).max(block.count);
            }
        }
    }

    let files: Vec<FileCoverage> = file_map
        .into_values()
        .map(|b| {
            let total_lines = b.line_hits.len();
            let covered_lines = b.line_hits.values().filter(|&&c| c > 0).count();
            FileCoverage {
                path: PathBuf::from(simplify_go_path(&b.path)),
                total_lines,
                covered_lines,
                uncovered_ranges: Vec::new(),
                line_hits: b.line_hits,
                total_branches: 0,
                covered_branches: 0,
            }
        })
        .collect();

    CoverageResult::from_files(files)
}

struct FileBuilder {
    path: String,
    line_hits: HashMap<usize, u64>,
}

struct CoverageBlock {
    file: String,
    start_line: usize,
    end_line: usize,
    count: u64,
}

/// Parse a single coverage profile line.
///
/// Format: `file:startline.startcol,endline.endcol numstmt count`
fn parse_coverage_line(line: &str) -> Option<CoverageBlock> {
    // Split from the right to handle filenames with spaces
    let colon_pos = line.rfind(':')?;
    let file = &line[..colon_pos];

    let rest = &line[colon_pos + 1..];
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }

    let range = parts[0]; // "10.14,12.2"
    let count: u64 = parts[2].parse().ok()?;

    let (start, end) = range.split_once(',')?;
    let start_line: usize = start.split('.').next()?.parse().ok()?;
    let end_line: usize = end.split('.').next()?.parse().ok()?;

    Some(CoverageBlock {
        file: file.to_string(),
        start_line,
        end_line,
        count,
    })
}

/// Simplify a Go module path to a relative path.
///
/// Converts `github.com/user/project/pkg/file.go` to `pkg/file.go`
/// by stripping the first 3 path segments (domain/user/project).
fn simplify_go_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 3 && parts[0].contains('.') {
        // Looks like a module path, strip domain/user/project
        parts[3..].join("/")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_profile() {
        let content = "\
mode: set
github.com/user/project/main.go:10.14,12.2 1 1
github.com/user/project/main.go:14.2,16.14 1 0
";
        let result = parse_go_cover(content);
        assert_eq!(result.files.len(), 1);
        // Lines 10,11,12 covered; 14,15,16 not covered
        assert_eq!(result.total_lines, 6);
        assert_eq!(result.covered_lines, 3);
    }

    #[test]
    fn parse_multiple_files() {
        let content = "\
mode: count
github.com/user/project/a.go:5.2,7.3 1 3
github.com/user/project/b.go:10.5,12.8 1 0
";
        let result = parse_go_cover(content);
        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn parse_count_mode() {
        let content = "\
mode: count
github.com/user/project/main.go:10.2,12.3 1 5
github.com/user/project/main.go:10.2,12.3 1 3
";
        let result = parse_go_cover(content);
        // Should take max count
        assert_eq!(result.files[0].line_hits.get(&10), Some(&5));
    }

    #[test]
    fn parse_empty() {
        let result = parse_go_cover("");
        assert_eq!(result.files.len(), 0);
    }

    #[test]
    fn parse_mode_only() {
        let result = parse_go_cover("mode: set\n");
        assert_eq!(result.files.len(), 0);
    }

    #[test]
    fn simplify_module_path() {
        assert_eq!(
            simplify_go_path("github.com/user/project/pkg/file.go"),
            "pkg/file.go"
        );
        assert_eq!(
            simplify_go_path("github.com/user/project/main.go"),
            "main.go"
        );
    }

    #[test]
    fn simplify_local_path() {
        assert_eq!(simplify_go_path("main.go"), "main.go");
        assert_eq!(simplify_go_path("pkg/util.go"), "pkg/util.go");
    }

    #[test]
    fn parse_coverage_line_valid() {
        let block = parse_coverage_line("github.com/user/project/main.go:10.14,12.2 1 5").unwrap();
        assert_eq!(block.file, "github.com/user/project/main.go");
        assert_eq!(block.start_line, 10);
        assert_eq!(block.end_line, 12);
        assert_eq!(block.count, 5);
    }

    #[test]
    fn parse_coverage_line_invalid() {
        assert!(parse_coverage_line("invalid line").is_none());
        assert!(parse_coverage_line("").is_none());
    }

    #[test]
    fn overlapping_blocks() {
        let content = "\
mode: set
github.com/user/project/main.go:5.2,10.3 1 1
github.com/user/project/main.go:8.2,12.3 1 1
";
        let result = parse_go_cover(content);
        // Lines 5-12, all covered (8-10 overlap, merged)
        assert_eq!(result.total_lines, 8);
        assert_eq!(result.covered_lines, 8);
    }

    #[test]
    fn zero_count_lines() {
        let content = "\
mode: set
github.com/user/project/main.go:5.2,7.3 1 0
";
        let result = parse_go_cover(content);
        assert_eq!(result.covered_lines, 0);
        assert_eq!(result.total_lines, 3);
    }
}
