use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, TestxError};

/// Get the list of files changed according to git.
///
/// Supports multiple diff modes:
/// - `head`: Changes in the current working tree vs HEAD (uncommitted changes)
/// - `staged`: Only staged changes (git diff --cached)
/// - `branch:name`: Changes vs a specific branch (e.g., `branch:main`)
/// - `commit:sha`: Changes since a specific commit
#[derive(Debug, Clone)]
pub enum DiffMode {
    /// Uncommitted changes (working tree + staged vs HEAD).
    Head,
    /// Only staged changes.
    Staged,
    /// Changes compared to a specific branch.
    Branch(String),
    /// Changes since a specific commit.
    Commit(String),
}

impl DiffMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "head" | "HEAD" => Ok(DiffMode::Head),
            "staged" | "STAGED" => Ok(DiffMode::Staged),
            s if s.starts_with("branch:") => {
                let branch = &s[7..];
                if branch.is_empty() {
                    return Err(TestxError::ConfigError {
                        message: "Branch name cannot be empty in 'branch:<name>'".into(),
                    });
                }
                Ok(DiffMode::Branch(branch.to_string()))
            }
            s if s.starts_with("commit:") => {
                let sha = &s[7..];
                if sha.is_empty() {
                    return Err(TestxError::ConfigError {
                        message: "Commit SHA cannot be empty in 'commit:<sha>'".into(),
                    });
                }
                Ok(DiffMode::Commit(sha.to_string()))
            }
            other => Err(TestxError::ConfigError {
                message: format!(
                    "Unknown diff mode '{}'. Use: head, staged, branch:<name>, commit:<sha>",
                    other
                ),
            }),
        }
    }

    pub fn description(&self) -> String {
        match self {
            DiffMode::Head => "uncommitted changes vs HEAD".to_string(),
            DiffMode::Staged => "staged changes".to_string(),
            DiffMode::Branch(b) => format!("changes vs branch '{}'", b),
            DiffMode::Commit(c) => format!("changes since commit '{}'", c),
        }
    }
}

/// Get changed files from git diff.
pub fn get_changed_files(project_dir: &Path, mode: &DiffMode) -> Result<Vec<PathBuf>> {
    let mut cmd = Command::new("git");
    cmd.current_dir(project_dir);

    match mode {
        DiffMode::Head => {
            // Show both staged and unstaged changes, plus untracked files
            cmd.args(["diff", "--name-only", "HEAD"]);
        }
        DiffMode::Staged => {
            cmd.args(["diff", "--name-only", "--cached"]);
        }
        DiffMode::Branch(branch) => {
            // Changes between current HEAD and the merge-base with branch
            cmd.args(["diff", "--name-only", &format!("{}...HEAD", branch)]);
        }
        DiffMode::Commit(sha) => {
            cmd.args(["diff", "--name-only", sha, "HEAD"]);
        }
    }

    let output = cmd.output().map_err(|e| TestxError::IoError {
        context: "Failed to run git diff".into(),
        source: e,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TestxError::ConfigError {
            message: format!("git diff failed: {}", stderr.trim()),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    // For HEAD mode, also include untracked files
    if matches!(mode, DiffMode::Head)
        && let Ok(untracked) = get_untracked_files(project_dir)
    {
        files.extend(untracked);
    }

    // Deduplicate
    let unique: HashSet<PathBuf> = files.into_iter().collect();
    let mut result: Vec<PathBuf> = unique.into_iter().collect();
    result.sort();

    Ok(result)
}

/// Get untracked files (not ignored).
fn get_untracked_files(project_dir: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .current_dir(project_dir)
        .args(["ls-files", "--others", "--exclude-standard"])
        .output()
        .map_err(|e| TestxError::IoError {
            context: "Failed to run git ls-files".into(),
            source: e,
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// Language extension mapping for determining if changed files affect tests.
struct LanguageExtensions {
    mappings: Vec<(&'static str, &'static [&'static str])>,
}

impl LanguageExtensions {
    fn new() -> Self {
        Self {
            mappings: vec![
                ("Rust", &["rs", "toml"]),
                ("Go", &["go", "mod", "sum"]),
                ("Python", &["py", "pyi", "cfg", "ini", "toml"]),
                (
                    "JavaScript",
                    &["js", "jsx", "ts", "tsx", "mjs", "cjs", "json"],
                ),
                (
                    "Java",
                    &["java", "kt", "kts", "gradle", "xml", "properties"],
                ),
                (
                    "C++",
                    &["cpp", "cc", "cxx", "c", "h", "hpp", "hxx", "cmake"],
                ),
                ("Ruby", &["rb", "rake", "gemspec"]),
                ("Elixir", &["ex", "exs"]),
                ("PHP", &["php", "xml"]),
                (".NET", &["cs", "fs", "vb", "csproj", "fsproj", "sln"]),
                ("Zig", &["zig"]),
            ],
        }
    }

    /// Check if a file extension is relevant for any adapter.
    fn is_relevant_extension(&self, extension: &str) -> bool {
        self.mappings
            .iter()
            .any(|(_, exts)| exts.contains(&extension))
    }

    /// Get the adapters that a file extension belongs to.
    fn adapters_for_extension(&self, extension: &str) -> Vec<&'static str> {
        self.mappings
            .iter()
            .filter(|(_, exts)| exts.contains(&extension))
            .map(|(adapter, _)| *adapter)
            .collect()
    }
}

/// Result of impact analysis.
#[derive(Debug, Clone)]
pub struct ImpactAnalysis {
    /// Total files changed.
    pub total_changed: usize,
    /// Files that are relevant to testing.
    pub relevant_files: Vec<PathBuf>,
    /// Files that are not relevant to testing.
    pub irrelevant_files: Vec<PathBuf>,
    /// Adapters (languages) that are affected.
    pub affected_adapters: Vec<String>,
    /// Whether tests should be run.
    pub should_run_tests: bool,
    /// The diff mode used.
    pub diff_mode: String,
}

/// Analyze changed files to determine test impact.
pub fn analyze_impact(project_dir: &Path, mode: &DiffMode) -> Result<ImpactAnalysis> {
    let changed_files = get_changed_files(project_dir, mode)?;
    let extensions = LanguageExtensions::new();

    // Paths to exclude from impact analysis (build artifacts, cache, etc.)
    let excluded_prefixes: &[&str] = &[".testx/", ".testx\\"];

    let mut relevant_files = Vec::new();
    let mut irrelevant_files = Vec::new();
    let mut affected_set: HashSet<String> = HashSet::new();

    for file in &changed_files {
        // Skip excluded paths
        let path_str = file.to_string_lossy();
        if excluded_prefixes.iter().any(|p| path_str.starts_with(p)) {
            irrelevant_files.push(file.clone());
            continue;
        }

        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

        if extensions.is_relevant_extension(ext) || is_config_file(file) {
            relevant_files.push(file.clone());
            for adapter in extensions.adapters_for_extension(ext) {
                affected_set.insert(adapter.to_string());
            }
            // Config files affect all adapters
            if is_config_file(file) {
                affected_set.insert("config".to_string());
            }
        } else {
            irrelevant_files.push(file.clone());
        }
    }

    let mut affected_adapters: Vec<String> = affected_set.into_iter().collect();
    affected_adapters.sort();

    let should_run_tests = !relevant_files.is_empty();

    Ok(ImpactAnalysis {
        total_changed: changed_files.len(),
        relevant_files,
        irrelevant_files,
        affected_adapters,
        should_run_tests,
        diff_mode: mode.description(),
    })
}

/// Check if a file is a project config/build file.
fn is_config_file(path: &Path) -> bool {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    matches!(
        filename,
        "Cargo.toml"
            | "Cargo.lock"
            | "go.mod"
            | "go.sum"
            | "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "Gemfile"
            | "Gemfile.lock"
            | "requirements.txt"
            | "setup.py"
            | "setup.cfg"
            | "pyproject.toml"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "mix.exs"
            | "composer.json"
            | "composer.lock"
            | "CMakeLists.txt"
            | "Makefile"
            | "testx.toml"
    )
}

/// Check if git is available and the project is a git repository.
pub fn is_git_repo(project_dir: &Path) -> bool {
    Command::new("git")
        .current_dir(project_dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Format impact analysis for display.
pub fn format_impact(analysis: &ImpactAnalysis) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Impact Analysis ({}): {} file(s) changed",
        analysis.diff_mode, analysis.total_changed
    ));

    if analysis.relevant_files.is_empty() {
        lines.push("  No test-relevant files changed — tests can be skipped.".to_string());
        return lines.join("\n");
    }

    lines.push(format!(
        "  {} relevant, {} irrelevant",
        analysis.relevant_files.len(),
        analysis.irrelevant_files.len()
    ));

    if !analysis.affected_adapters.is_empty() {
        lines.push(format!(
            "  Affected: {}",
            analysis.affected_adapters.join(", ")
        ));
    }

    lines.push("  Changed test-relevant files:".to_string());
    for file in &analysis.relevant_files {
        lines.push(format!("    {}", file.display()));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_mode_parse_head() {
        let mode = DiffMode::parse("head").unwrap();
        assert!(matches!(mode, DiffMode::Head));

        let mode = DiffMode::parse("HEAD").unwrap();
        assert!(matches!(mode, DiffMode::Head));
    }

    #[test]
    fn diff_mode_parse_staged() {
        let mode = DiffMode::parse("staged").unwrap();
        assert!(matches!(mode, DiffMode::Staged));
    }

    #[test]
    fn diff_mode_parse_branch() {
        let mode = DiffMode::parse("branch:main").unwrap();
        match mode {
            DiffMode::Branch(b) => assert_eq!(b, "main"),
            _ => panic!("Expected Branch"),
        }
    }

    #[test]
    fn diff_mode_parse_commit() {
        let mode = DiffMode::parse("commit:abc123").unwrap();
        match mode {
            DiffMode::Commit(c) => assert_eq!(c, "abc123"),
            _ => panic!("Expected Commit"),
        }
    }

    #[test]
    fn diff_mode_parse_errors() {
        assert!(DiffMode::parse("invalid").is_err());
        assert!(DiffMode::parse("branch:").is_err());
        assert!(DiffMode::parse("commit:").is_err());
    }

    #[test]
    fn diff_mode_description() {
        assert_eq!(DiffMode::Head.description(), "uncommitted changes vs HEAD");
        assert_eq!(DiffMode::Staged.description(), "staged changes");
        assert_eq!(
            DiffMode::Branch("main".into()).description(),
            "changes vs branch 'main'"
        );
        assert_eq!(
            DiffMode::Commit("abc".into()).description(),
            "changes since commit 'abc'"
        );
    }

    #[test]
    fn language_extensions_rust() {
        let exts = LanguageExtensions::new();
        assert!(exts.is_relevant_extension("rs"));
        assert!(exts.is_relevant_extension("toml"));
        let adapters = exts.adapters_for_extension("rs");
        assert!(adapters.contains(&"Rust"));
    }

    #[test]
    fn language_extensions_go() {
        let exts = LanguageExtensions::new();
        assert!(exts.is_relevant_extension("go"));
        let adapters = exts.adapters_for_extension("go");
        assert!(adapters.contains(&"Go"));
    }

    #[test]
    fn language_extensions_javascript() {
        let exts = LanguageExtensions::new();
        for ext in &["js", "jsx", "ts", "tsx", "mjs", "cjs"] {
            assert!(exts.is_relevant_extension(ext));
            let adapters = exts.adapters_for_extension(ext);
            assert!(adapters.contains(&"JavaScript"));
        }
    }

    #[test]
    fn language_extensions_all_languages() {
        let exts = LanguageExtensions::new();
        let test_cases = vec![
            ("py", "Python"),
            ("java", "Java"),
            ("cpp", "C++"),
            ("rb", "Ruby"),
            ("ex", "Elixir"),
            ("php", "PHP"),
            ("cs", ".NET"),
            ("zig", "Zig"),
        ];

        for (ext, adapter) in test_cases {
            assert!(
                exts.is_relevant_extension(ext),
                "Extension {} should be relevant",
                ext
            );
            let adapters = exts.adapters_for_extension(ext);
            assert!(
                adapters.contains(&adapter),
                "Extension {} should map to adapter {}",
                ext,
                adapter
            );
        }
    }

    #[test]
    fn irrelevant_extensions() {
        let exts = LanguageExtensions::new();
        assert!(!exts.is_relevant_extension("md"));
        assert!(!exts.is_relevant_extension("txt"));
        assert!(!exts.is_relevant_extension("png"));
        assert!(!exts.is_relevant_extension("yml"));
        assert!(!exts.is_relevant_extension(""));
    }

    #[test]
    fn config_file_detection() {
        assert!(is_config_file(Path::new("Cargo.toml")));
        assert!(is_config_file(Path::new("package.json")));
        assert!(is_config_file(Path::new("go.mod")));
        assert!(is_config_file(Path::new("requirements.txt")));
        assert!(is_config_file(Path::new("testx.toml")));
        assert!(is_config_file(Path::new("pom.xml")));
        assert!(is_config_file(Path::new("mix.exs")));
        assert!(is_config_file(Path::new("CMakeLists.txt")));

        assert!(!is_config_file(Path::new("README.md")));
        assert!(!is_config_file(Path::new("src/main.rs")));
        assert!(!is_config_file(Path::new("image.png")));
    }

    #[test]
    fn format_impact_no_relevant() {
        let analysis = ImpactAnalysis {
            total_changed: 3,
            relevant_files: vec![],
            irrelevant_files: vec![
                PathBuf::from("README.md"),
                PathBuf::from("docs/guide.md"),
                PathBuf::from(".gitignore"),
            ],
            affected_adapters: vec![],
            should_run_tests: false,
            diff_mode: "uncommitted changes vs HEAD".to_string(),
        };

        let output = format_impact(&analysis);
        assert!(output.contains("3 file(s) changed"));
        assert!(output.contains("tests can be skipped"));
    }

    #[test]
    fn format_impact_with_relevant() {
        let analysis = ImpactAnalysis {
            total_changed: 5,
            relevant_files: vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")],
            irrelevant_files: vec![
                PathBuf::from("README.md"),
                PathBuf::from("docs/api.md"),
                PathBuf::from(".gitignore"),
            ],
            affected_adapters: vec!["Rust".to_string()],
            should_run_tests: true,
            diff_mode: "changes vs branch 'main'".to_string(),
        };

        let output = format_impact(&analysis);
        assert!(output.contains("5 file(s) changed"));
        assert!(output.contains("2 relevant"));
        assert!(output.contains("3 irrelevant"));
        assert!(output.contains("Rust"));
        assert!(output.contains("src/main.rs"));
    }

    #[test]
    fn is_git_repo_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        // A fresh tempdir is not a git repo
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn impact_analysis_on_non_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = analyze_impact(dir.path(), &DiffMode::Head);
        // Should error because not a git repo
        assert!(result.is_err());
    }
}
