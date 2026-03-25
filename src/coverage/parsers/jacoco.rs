//! JaCoCo XML coverage parser.
//!
//! Parses JaCoCo XML reports commonly produced by Java (Maven/Gradle)
//! and Kotlin projects.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::coverage::{CoverageResult, FileCoverage};

/// Parse JaCoCo XML coverage data.
///
/// Expected structure:
/// ```xml
/// <report name="project">
///   <package name="com/example">
///     <class name="com/example/MyClass" sourcefilename="MyClass.java">
///       <method name="doStuff" ...>
///         <counter type="LINE" missed="2" covered="5"/>
///         <counter type="BRANCH" missed="1" covered="3"/>
///       </method>
///     </class>
///     <sourcefile name="MyClass.java">
///       <line nr="10" mi="0" ci="3" mb="0" cb="2"/>
///       <line nr="11" mi="2" ci="0" mb="1" cb="0"/>
///       <counter type="LINE" missed="2" covered="5"/>
///     </sourcefile>
///   </package>
/// </report>
/// ```
pub fn parse_jacoco(content: &str) -> CoverageResult {
    let mut files = Vec::new();
    let mut current_package: Option<String> = None;
    let mut current_sourcefile: Option<String> = None;
    let mut line_hits: HashMap<usize, u64> = HashMap::new();
    let mut total_branches: usize = 0;
    let mut covered_branches: usize = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track package
        if trimmed.starts_with("<package ") {
            if let Some(name) = extract_attr_value(trimmed, "name") {
                current_package = Some(name.replace('/', std::path::MAIN_SEPARATOR_STR));
            }
        } else if trimmed == "</package>" {
            current_package = None;
        }

        // Track sourcefile
        if trimmed.starts_with("<sourcefile ")
            && let Some(name) = extract_attr_value(trimmed, "name") {
                current_sourcefile = Some(name);
                line_hits.clear();
                total_branches = 0;
                covered_branches = 0;
            }

        // Parse line data
        if trimmed.starts_with("<line ") && current_sourcefile.is_some()
            && let Some(nr) = extract_attr_value(trimmed, "nr")
                && let Ok(line_num) = nr.parse::<usize>() {
                    // ci = covered instructions, mi = missed instructions
                    let ci: u64 = extract_attr_value(trimmed, "ci")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    line_hits.insert(line_num, ci);

                    // mb = missed branches, cb = covered branches
                    let mb: usize = extract_attr_value(trimmed, "mb")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let cb: usize = extract_attr_value(trimmed, "cb")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    total_branches += mb + cb;
                    covered_branches += cb;
                }

        // End of sourcefile
        if (trimmed == "</sourcefile>" || trimmed.starts_with("</sourcefile>"))
            && let Some(name) = current_sourcefile.take() {
                let path = match &current_package {
                    Some(pkg) => PathBuf::from(pkg).join(&name),
                    None => PathBuf::from(&name),
                };

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
                line_hits.clear();
                total_branches = 0;
                covered_branches = 0;
            }
    }

    CoverageResult::from_files(files)
}

/// Extract an attribute value from an XML element.
fn extract_attr_value(line: &str, attr: &str) -> Option<String> {
    let search = format!("{attr}=\"");
    let start = line.find(&search)?;
    let value_start = start + search.len();
    let rest = &line[value_start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_jacoco() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<report name="testproject">
  <package name="com/example">
    <sourcefile name="MyClass.java">
      <line nr="10" mi="0" ci="3" mb="0" cb="0"/>
      <line nr="11" mi="2" ci="0" mb="0" cb="0"/>
      <line nr="12" mi="0" ci="1" mb="0" cb="0"/>
      <counter type="LINE" missed="1" covered="2"/>
    </sourcefile>
  </package>
</report>"#;

        let result = parse_jacoco(xml);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.total_lines, 3);
        assert_eq!(result.covered_lines, 2);
    }

    #[test]
    fn parse_with_branches() {
        let xml = r#"<report name="test">
  <package name="com/example">
    <sourcefile name="Logic.java">
      <line nr="10" mi="0" ci="3" mb="1" cb="2"/>
      <line nr="11" mi="0" ci="1" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;

        let result = parse_jacoco(xml);
        assert_eq!(result.total_branches, 3);
        assert_eq!(result.covered_branches, 2);
    }

    #[test]
    fn parse_multiple_sourcefiles() {
        let xml = r#"<report name="test">
  <package name="com/example">
    <sourcefile name="A.java">
      <line nr="1" mi="0" ci="1" mb="0" cb="0"/>
    </sourcefile>
    <sourcefile name="B.java">
      <line nr="1" mi="0" ci="1" mb="0" cb="0"/>
      <line nr="2" mi="1" ci="0" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;

        let result = parse_jacoco(xml);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.total_lines, 3);
        assert_eq!(result.covered_lines, 2);
    }

    #[test]
    fn parse_package_path() {
        let xml = r#"<report name="test">
  <package name="com/example/util">
    <sourcefile name="Helper.java">
      <line nr="1" mi="0" ci="1" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;

        let result = parse_jacoco(xml);
        // Path should combine package and filename
        let path = &result.files[0].path;
        assert!(
            path.to_string_lossy().contains("com")
                && path.to_string_lossy().contains("example")
                && path.to_string_lossy().contains("Helper.java")
        );
    }

    #[test]
    fn parse_no_package() {
        let xml = r#"<report name="test">
  <sourcefile name="Main.java">
    <line nr="1" mi="0" ci="1" mb="0" cb="0"/>
  </sourcefile>
</report>"#;

        let result = parse_jacoco(xml);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, PathBuf::from("Main.java"));
    }

    #[test]
    fn parse_empty() {
        let result = parse_jacoco("");
        assert_eq!(result.files.len(), 0);
        assert_eq!(result.percentage, 0.0);
    }

    #[test]
    fn parse_all_missed() {
        let xml = r#"<report name="test">
  <package name="pkg">
    <sourcefile name="Test.java">
      <line nr="1" mi="5" ci="0" mb="2" cb="0"/>
      <line nr="2" mi="3" ci="0" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;

        let result = parse_jacoco(xml);
        assert_eq!(result.covered_lines, 0);
        assert_eq!(result.total_lines, 2);
        assert_eq!(result.covered_branches, 0);
    }

    #[test]
    fn extract_attr() {
        assert_eq!(
            extract_attr_value(r#"<package name="com/example">"#, "name"),
            Some("com/example".into())
        );
        assert_eq!(
            extract_attr_value(r#"<line nr="42" ci="3"/>"#, "nr"),
            Some("42".into())
        );
    }
}
