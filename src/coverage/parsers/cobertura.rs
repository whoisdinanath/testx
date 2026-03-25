//! Cobertura XML coverage parser.
//!
//! Parses Cobertura XML format used by Python coverage.py,
//! Istanbul (JS), PHPUnit, and many CI systems.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::coverage::{CoverageResult, FileCoverage};

/// Parse Cobertura XML coverage data.
///
/// Expected structure:
/// ```xml
/// <coverage line-rate="0.85" branch-rate="0.70">
///   <packages>
///     <package name="..." line-rate="..." branch-rate="...">
///       <classes>
///         <class name="..." filename="..." line-rate="...">
///           <lines>
///             <line number="1" hits="5"/>
///             <line number="2" hits="0" branch="true" condition-coverage="50% (1/2)"/>
///           </lines>
///         </class>
///       </classes>
///     </package>
///   </packages>
/// </coverage>
/// ```
pub fn parse_cobertura(content: &str) -> CoverageResult {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut line_hits: HashMap<usize, u64> = HashMap::new();
    let mut total_branches: usize = 0;
    let mut covered_branches: usize = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect <class> with filename
        if let Some(filename) = extract_attr(trimmed, "class", "filename") {
            // Finalize previous file if any
            if let Some(path) = current_path.take() {
                finalize_file(&mut files, path, &line_hits, total_branches, covered_branches);
            }
            current_path = Some(filename);
            line_hits.clear();
            total_branches = 0;
            covered_branches = 0;
        }

        // Parse <line> elements
        if trimmed.starts_with("<line ") || trimmed.starts_with("<line>") {
            if let (Some(number), Some(hits)) = (
                extract_attr_value(trimmed, "number"),
                extract_attr_value(trimmed, "hits"),
            )
                && let (Ok(num), Ok(hit_count)) = (number.parse::<usize>(), hits.parse::<u64>()) {
                    let entry = line_hits.entry(num).or_insert(0);
                    *entry = (*entry).max(hit_count);
                }

            // Check for branch coverage
            if let Some(branch) = extract_attr_value(trimmed, "branch")
                && branch == "true"
                    && let Some(cond) = extract_attr_value(trimmed, "condition-coverage") {
                        let (br_total, br_covered) = parse_condition_coverage(&cond);
                        total_branches += br_total;
                        covered_branches += br_covered;
                    }
        }

        // End of class
        if (trimmed == "</class>" || trimmed.starts_with("</class>"))
            && let Some(path) = current_path.take() {
                finalize_file(&mut files, path, &line_hits, total_branches, covered_branches);
                line_hits.clear();
                total_branches = 0;
                covered_branches = 0;
            }
    }

    // Handle leftover
    if let Some(path) = current_path.take() {
        finalize_file(&mut files, path, &line_hits, total_branches, covered_branches);
    }

    CoverageResult::from_files(files)
}

fn finalize_file(
    files: &mut Vec<FileCoverage>,
    path: String,
    line_hits: &HashMap<usize, u64>,
    total_branches: usize,
    covered_branches: usize,
) {
    let total_lines = line_hits.len();
    let covered_lines = line_hits.values().filter(|&&c| c > 0).count();

    files.push(FileCoverage {
        path: PathBuf::from(path),
        total_lines,
        covered_lines,
        uncovered_ranges: Vec::new(),
        line_hits: line_hits.clone(),
        total_branches,
        covered_branches,
    });
}

/// Extract an attribute value from a specific XML element.
fn extract_attr(line: &str, element: &str, attr: &str) -> Option<String> {
    let open_tag = format!("<{element} ");
    if !line.contains(&open_tag) {
        return None;
    }
    extract_attr_value(line, attr)
}

/// Extract an attribute value from an XML line.
fn extract_attr_value(line: &str, attr: &str) -> Option<String> {
    let search = format!("{attr}=\"");
    let start = line.find(&search)?;
    let value_start = start + search.len();
    let rest = &line[value_start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse condition-coverage string like "50% (1/2)" into (total, covered).
fn parse_condition_coverage(cond: &str) -> (usize, usize) {
    // Format: "50% (1/2)"
    if let Some(paren_start) = cond.find('(')
        && let Some(paren_end) = cond.find(')') {
            let inner = &cond[paren_start + 1..paren_end];
            if let Some((covered_str, total_str)) = inner.split_once('/') {
                let covered = covered_str.trim().parse().unwrap_or(0);
                let total = total_str.trim().parse().unwrap_or(0);
                return (total, covered);
            }
        }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_cobertura() {
        let xml = r#"<?xml version="1.0" ?>
<coverage version="5.5" timestamp="1234567" line-rate="0.75" branch-rate="0">
  <packages>
    <package name="src" line-rate="0.75">
      <classes>
        <class name="main" filename="src/main.py" line-rate="0.75">
          <lines>
            <line number="1" hits="5"/>
            <line number="2" hits="3"/>
            <line number="3" hits="0"/>
            <line number="4" hits="1"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;

        let result = parse_cobertura(xml);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.total_lines, 4);
        assert_eq!(result.covered_lines, 3);
        assert!((result.percentage - 75.0).abs() < 0.1);
    }

    #[test]
    fn parse_multiple_classes() {
        let xml = r#"<coverage>
  <packages>
    <package name="pkg">
      <classes>
        <class name="A" filename="a.py">
          <lines>
            <line number="1" hits="1"/>
            <line number="2" hits="1"/>
          </lines>
        </class>
        <class name="B" filename="b.py">
          <lines>
            <line number="1" hits="0"/>
            <line number="2" hits="0"/>
            <line number="3" hits="1"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;

        let result = parse_cobertura(xml);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.total_lines, 5);
        assert_eq!(result.covered_lines, 3);
    }

    #[test]
    fn parse_with_branches() {
        let xml = r#"<coverage>
  <packages>
    <package name="pkg">
      <classes>
        <class name="A" filename="a.py">
          <lines>
            <line number="1" hits="1" branch="true" condition-coverage="50% (1/2)"/>
            <line number="2" hits="1"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;

        let result = parse_cobertura(xml);
        assert_eq!(result.total_branches, 2);
        assert_eq!(result.covered_branches, 1);
    }

    #[test]
    fn parse_empty() {
        let result = parse_cobertura("");
        assert_eq!(result.files.len(), 0);
        assert_eq!(result.percentage, 0.0);
    }

    #[test]
    fn extract_attr_value_test() {
        assert_eq!(
            extract_attr_value(r#"<class name="Foo" filename="foo.py">"#, "filename"),
            Some("foo.py".into())
        );
        assert_eq!(
            extract_attr_value(r#"<line number="42" hits="5"/>"#, "number"),
            Some("42".into())
        );
        assert_eq!(
            extract_attr_value(r#"<line number="1"/>"#, "hits"),
            None
        );
    }

    #[test]
    fn parse_condition_coverage_test() {
        assert_eq!(parse_condition_coverage("50% (1/2)"), (2, 1));
        assert_eq!(parse_condition_coverage("100% (4/4)"), (4, 4));
        assert_eq!(parse_condition_coverage("0% (0/3)"), (3, 0));
        assert_eq!(parse_condition_coverage("invalid"), (0, 0));
    }

    #[test]
    fn filename_preserved() {
        let xml = r#"<coverage>
  <packages>
    <package name="pkg">
      <classes>
        <class name="A" filename="src/deep/nested/file.py">
          <lines>
            <line number="1" hits="1"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;

        let result = parse_cobertura(xml);
        assert_eq!(
            result.files[0].path,
            PathBuf::from("src/deep/nested/file.py")
        );
    }
}
