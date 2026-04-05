use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::TestRunResult;
use crate::detection::DetectionEngine;

/// Directories to skip during recursive workspace scanning.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "vendor",
    "venv",
    ".venv",
    "__pycache__",
    ".tox",
    ".nox",
    ".mypy_cache",
    ".pytest_cache",
    ".eggs",
    "coverage",
    ".coverage",
    "htmlcov",
    ".gradle",
    ".maven",
    ".idea",
    ".vscode",
    "bin",
    "obj",
    "packages",
    "zig-cache",
    "zig-out",
    "_build",
    "deps",
    ".elixir_ls",
    ".bundle",
    ".cache",
    ".cargo",
    ".rustup",
];

/// A project discovered within a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceProject {
    /// Path to the project root directory.
    pub path: PathBuf,
    /// Detection result — language, framework, confidence.
    pub language: String,
    /// Framework name.
    pub framework: String,
    /// Confidence score.
    pub confidence: f64,
    /// Index of the adapter in the detection engine.
    pub adapter_index: usize,
}

/// Result of running tests in a single workspace project.
#[derive(Debug, Clone)]
pub struct WorkspaceRunResult {
    pub project: WorkspaceProject,
    pub result: Option<TestRunResult>,
    pub duration: Duration,
    pub error: Option<String>,
    pub skipped: bool,
}

/// Aggregated results across all workspace projects.
#[derive(Debug, Clone)]
pub struct WorkspaceReport {
    pub results: Vec<WorkspaceRunResult>,
    pub total_duration: Duration,
    pub projects_found: usize,
    pub projects_run: usize,
    pub projects_passed: usize,
    pub projects_failed: usize,
    pub projects_skipped: usize,
    pub total_tests: usize,
    pub total_passed: usize,
    pub total_failed: usize,
}

/// Configuration for workspace scanning and execution.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Maximum directory depth to scan (0 = unlimited).
    pub max_depth: usize,
    /// Run projects in parallel.
    pub parallel: bool,
    /// Maximum parallel jobs (0 = auto-detect CPU count).
    pub max_jobs: usize,
    /// Fail fast — stop on first project failure.
    pub fail_fast: bool,
    /// Filter to specific languages.
    pub filter_languages: Vec<String>,
    /// Custom directories to skip.
    pub skip_dirs: Vec<String>,
    /// Directories to include even if they're in the default skip list.
    /// Overrides SKIP_DIRS for specific directory names (e.g., "packages").
    pub include_dirs: Vec<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            parallel: true,
            max_jobs: 0,
            fail_fast: false,
            filter_languages: Vec::new(),
            skip_dirs: Vec::new(),
            include_dirs: Vec::new(),
        }
    }
}

impl WorkspaceConfig {
    pub fn effective_jobs(&self) -> usize {
        if self.max_jobs == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            self.max_jobs
        }
    }
}

/// Recursively scan a directory tree to discover all testable projects.
///
/// Returns projects sorted by path. Each project is the "deepest" match —
/// if `/repo/services/api` has a Cargo.toml and `/repo` also has one,
/// both are returned as separate projects.
pub fn discover_projects(
    root: &Path,
    engine: &DetectionEngine,
    config: &WorkspaceConfig,
) -> Vec<WorkspaceProject> {
    let mut skip_set: HashSet<&str> = SKIP_DIRS.iter().copied().collect();
    let custom_skip: HashSet<String> = config.skip_dirs.iter().cloned().collect();

    // Allow include_dirs to override default skip list
    for dir in &config.include_dirs {
        skip_set.remove(dir.as_str());
    }

    let mut projects = Vec::new();
    let mut visited = HashSet::new();

    scan_dir(
        root,
        engine,
        config,
        &skip_set,
        &custom_skip,
        0,
        &mut projects,
        &mut visited,
    );

    // Sort by path for deterministic ordering
    projects.sort_by(|a, b| a.path.cmp(&b.path));

    // Apply language filter if specified
    if !config.filter_languages.is_empty() {
        projects.retain(|p| {
            config
                .filter_languages
                .iter()
                .any(|lang| p.language.to_lowercase().contains(&lang.to_lowercase()))
        });
    }

    projects
}

#[allow(clippy::too_many_arguments)]
fn scan_dir(
    dir: &Path,
    engine: &DetectionEngine,
    config: &WorkspaceConfig,
    skip_set: &HashSet<&str>,
    custom_skip: &HashSet<String>,
    depth: usize,
    projects: &mut Vec<WorkspaceProject>,
    visited: &mut HashSet<PathBuf>,
) {
    // Depth limit
    if config.max_depth > 0 && depth > config.max_depth {
        return;
    }

    // Canonicalize to avoid symlink loops
    let canonical = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return,
    };
    if !visited.insert(canonical.clone()) {
        return;
    }

    // Try to detect a project in this directory
    if let Some(detected) = engine.detect(dir) {
        projects.push(WorkspaceProject {
            path: dir.to_path_buf(),
            language: detected.detection.language.clone(),
            framework: detected.detection.framework.clone(),
            confidence: detected.detection.confidence as f64,
            adapter_index: detected.adapter_index,
        });
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let dir_name = match entry_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Skip hidden directories (starting with .)
        if dir_name.starts_with('.') {
            continue;
        }

        // Skip known non-project directories
        if skip_set.contains(dir_name.as_str()) {
            continue;
        }

        // Skip custom directories
        if custom_skip.contains(&dir_name) {
            continue;
        }

        scan_dir(
            &entry_path,
            engine,
            config,
            skip_set,
            custom_skip,
            depth + 1,
            projects,
            visited,
        );
    }
}

/// Run tests in all discovered workspace projects.
pub fn run_workspace(
    projects: &[WorkspaceProject],
    engine: &DetectionEngine,
    extra_args: &[String],
    config: &WorkspaceConfig,
    env_vars: &[(String, String)],
    verbose: bool,
) -> WorkspaceReport {
    let start = Instant::now();

    let results: Vec<WorkspaceRunResult> = if config.parallel && projects.len() > 1 {
        run_parallel(projects, engine, extra_args, config, env_vars, verbose)
    } else {
        run_sequential(projects, engine, extra_args, config, env_vars, verbose)
    };

    build_report(results, projects.len(), start.elapsed())
}

fn run_sequential(
    projects: &[WorkspaceProject],
    engine: &DetectionEngine,
    extra_args: &[String],
    config: &WorkspaceConfig,
    env_vars: &[(String, String)],
    verbose: bool,
) -> Vec<WorkspaceRunResult> {
    let mut results = Vec::new();

    for project in projects {
        let result = run_single_project(project, engine, extra_args, env_vars, verbose);

        let failed =
            result.result.as_ref().is_some_and(|r| r.total_failed() > 0) || result.error.is_some();

        results.push(result);

        if config.fail_fast && failed {
            // Mark remaining projects as skipped
            for remaining in projects.iter().skip(results.len()) {
                results.push(WorkspaceRunResult {
                    project: remaining.clone(),
                    result: None,
                    duration: Duration::ZERO,
                    error: None,
                    skipped: true,
                });
            }
            break;
        }
    }

    results
}

fn run_parallel(
    projects: &[WorkspaceProject],
    engine: &DetectionEngine,
    extra_args: &[String],
    config: &WorkspaceConfig,
    env_vars: &[(String, String)],
    _verbose: bool,
) -> Vec<WorkspaceRunResult> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let jobs = config.effective_jobs().min(projects.len());
    let cancelled = Arc::new(AtomicBool::new(false));
    let fail_fast = config.fail_fast;

    // Build all commands before spawning threads (engine/adapters are not Send/Sync)
    let mut project_commands: Vec<(usize, WorkspaceProject, Option<std::process::Command>)> =
        Vec::new();

    for (i, project) in projects.iter().enumerate() {
        let adapter = engine.adapter(project.adapter_index);
        match adapter.build_command(&project.path, extra_args) {
            Ok(mut cmd) => {
                for (key, value) in env_vars {
                    cmd.env(key, value);
                }
                project_commands.push((i, project.clone(), Some(cmd)));
            }
            Err(_) => {
                project_commands.push((i, project.clone(), None));
            }
        }
    }

    // Thread result: either raw output to parse, or an error/skip
    #[derive(Debug)]
    enum ThreadResult {
        RawOutput {
            idx: usize,
            project: WorkspaceProject,
            stdout: String,
            stderr: String,
            exit_code: i32,
            elapsed: Duration,
        },
        Error {
            idx: usize,
            project: WorkspaceProject,
            error: String,
            elapsed: Duration,
        },
        Skipped {
            idx: usize,
            project: WorkspaceProject,
        },
    }

    let results: Arc<Mutex<Vec<ThreadResult>>> = Arc::new(Mutex::new(Vec::new()));

    // Partition work into chunks across threads
    let mut chunks: Vec<Vec<(usize, WorkspaceProject, Option<std::process::Command>)>> =
        (0..jobs).map(|_| Vec::new()).collect();
    for (i, item) in project_commands.into_iter().enumerate() {
        chunks[i % jobs].push(item);
    }

    std::thread::scope(|scope| {
        for chunk in chunks {
            let results_ref = Arc::clone(&results);
            let cancelled_ref = Arc::clone(&cancelled);

            scope.spawn(move || {
                for (idx, project, cmd_opt) in chunk {
                    if cancelled_ref.load(Ordering::SeqCst) {
                        results_ref
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(ThreadResult::Skipped { idx, project });
                        continue;
                    }

                    let mut cmd = match cmd_opt {
                        Some(c) => c,
                        None => {
                            if fail_fast {
                                cancelled_ref.store(true, Ordering::SeqCst);
                            }
                            results_ref.lock().unwrap_or_else(|e| e.into_inner()).push(
                                ThreadResult::Error {
                                    idx,
                                    project,
                                    error: "Failed to build command".to_string(),
                                    elapsed: Duration::ZERO,
                                },
                            );
                            continue;
                        }
                    };

                    let start = Instant::now();
                    match cmd.output() {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                            let exit_code = output.status.code().unwrap_or(1);
                            let elapsed = start.elapsed();

                            if fail_fast && exit_code != 0 {
                                cancelled_ref.store(true, Ordering::SeqCst);
                            }

                            results_ref.lock().unwrap_or_else(|e| e.into_inner()).push(
                                ThreadResult::RawOutput {
                                    idx,
                                    project,
                                    stdout,
                                    stderr,
                                    exit_code,
                                    elapsed,
                                },
                            );
                        }
                        Err(e) => {
                            let elapsed = start.elapsed();
                            if fail_fast {
                                cancelled_ref.store(true, Ordering::SeqCst);
                            }
                            results_ref.lock().unwrap_or_else(|e| e.into_inner()).push(
                                ThreadResult::Error {
                                    idx,
                                    project,
                                    error: e.to_string(),
                                    elapsed,
                                },
                            );
                        }
                    }
                }
            });
        }
    });

    // Parse output on the main thread where we have access to the engine
    let mut raw_results: Vec<ThreadResult> = match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(arc) => arc
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect(),
    };

    // Convert ThreadResults into WorkspaceRunResults (parsing happens here)
    let mut final_results: Vec<(usize, WorkspaceRunResult)> = raw_results
        .drain(..)
        .map(|tr| match tr {
            ThreadResult::RawOutput {
                idx,
                project,
                stdout,
                stderr,
                exit_code,
                elapsed,
            } => {
                let adapter = engine.adapter(project.adapter_index);
                let mut parsed = adapter.parse_output(&stdout, &stderr, exit_code);
                if parsed.duration.as_millis() == 0 {
                    parsed.duration = elapsed;
                }
                (
                    idx,
                    WorkspaceRunResult {
                        project,
                        result: Some(parsed),
                        duration: elapsed,
                        error: None,
                        skipped: false,
                    },
                )
            }
            ThreadResult::Error {
                idx,
                project,
                error,
                elapsed,
            } => (
                idx,
                WorkspaceRunResult {
                    project,
                    result: None,
                    duration: elapsed,
                    error: Some(error),
                    skipped: false,
                },
            ),
            ThreadResult::Skipped { idx, project } => (
                idx,
                WorkspaceRunResult {
                    project,
                    result: None,
                    duration: Duration::ZERO,
                    error: None,
                    skipped: true,
                },
            ),
        })
        .collect();

    final_results.sort_by_key(|(idx, _)| *idx);
    final_results.into_iter().map(|(_, r)| r).collect()
}

fn run_single_project(
    project: &WorkspaceProject,
    engine: &DetectionEngine,
    extra_args: &[String],
    env_vars: &[(String, String)],
    _verbose: bool,
) -> WorkspaceRunResult {
    let adapter = engine.adapter(project.adapter_index);

    // Check if runner is available
    if let Some(missing) = adapter.check_runner() {
        return WorkspaceRunResult {
            project: project.clone(),
            result: None,
            duration: Duration::ZERO,
            error: Some(format!("Test runner '{}' not found", missing)),
            skipped: false,
        };
    }

    let start = Instant::now();

    let mut cmd = match adapter.build_command(&project.path, extra_args) {
        Ok(cmd) => cmd,
        Err(e) => {
            return WorkspaceRunResult {
                project: project.clone(),
                result: None,
                duration: start.elapsed(),
                error: Some(format!("Failed to build command: {}", e)),
                skipped: false,
            };
        }
    };

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code().unwrap_or(1);

            let mut result = adapter.parse_output(&stdout, &stderr, exit_code);
            let elapsed = start.elapsed();
            if result.duration.as_millis() == 0 {
                result.duration = elapsed;
            }

            WorkspaceRunResult {
                project: project.clone(),
                result: Some(result),
                duration: elapsed,
                error: None,
                skipped: false,
            }
        }
        Err(e) => WorkspaceRunResult {
            project: project.clone(),
            result: None,
            duration: start.elapsed(),
            error: Some(e.to_string()),
            skipped: false,
        },
    }
}

fn build_report(
    results: Vec<WorkspaceRunResult>,
    projects_found: usize,
    total_duration: Duration,
) -> WorkspaceReport {
    let projects_run = results
        .iter()
        .filter(|r| !r.skipped && r.error.is_none())
        .count();
    let projects_passed = results
        .iter()
        .filter(|r| r.result.as_ref().is_some_and(|res| res.is_success()))
        .count();
    let projects_failed = results
        .iter()
        .filter(|r| r.result.as_ref().is_some_and(|res| !res.is_success()) || r.error.is_some())
        .count();
    let projects_skipped = results.iter().filter(|r| r.skipped).count();

    let total_tests: usize = results
        .iter()
        .filter_map(|r| r.result.as_ref())
        .map(|r| r.total_tests())
        .sum();
    let total_passed: usize = results
        .iter()
        .filter_map(|r| r.result.as_ref())
        .map(|r| r.total_passed())
        .sum();
    let total_failed: usize = results
        .iter()
        .filter_map(|r| r.result.as_ref())
        .map(|r| r.total_failed())
        .sum();

    WorkspaceReport {
        results,
        total_duration,
        projects_found,
        projects_run,
        projects_passed,
        projects_failed,
        projects_skipped,
        total_tests,
        total_passed,
        total_failed,
    }
}

/// Format a workspace report for terminal display.
pub fn format_workspace_report(report: &WorkspaceReport) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "  {} projects found, {} run, {} passed, {} failed{}",
        report.projects_found,
        report.projects_run,
        report.projects_passed,
        report.projects_failed,
        if report.projects_skipped > 0 {
            format!(", {} skipped", report.projects_skipped)
        } else {
            String::new()
        }
    ));
    lines.push(String::new());

    for run_result in &report.results {
        let rel_path = run_result
            .project
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| run_result.project.path.display().to_string());

        if run_result.skipped {
            lines.push(format!(
                "  {} {} ({}) — skipped",
                "○", rel_path, run_result.project.language,
            ));
            continue;
        }

        if let Some(ref error) = run_result.error {
            lines.push(format!(
                "  {} {} ({}) — error: {}",
                "✗", rel_path, run_result.project.language, error,
            ));
            continue;
        }

        if let Some(ref result) = run_result.result {
            let icon = if result.is_success() { "✓" } else { "✗" };
            let status = if result.is_success() { "PASS" } else { "FAIL" };
            lines.push(format!(
                "  {} {} ({}) — {} ({} passed, {} failed, {} total) in {:.1}s",
                icon,
                rel_path,
                run_result.project.language,
                status,
                result.total_passed(),
                result.total_failed(),
                result.total_tests(),
                run_result.duration.as_secs_f64(),
            ));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "  Total: {} tests, {} passed, {} failed in {:.2}s",
        report.total_tests,
        report.total_passed,
        report.total_failed,
        report.total_duration.as_secs_f64(),
    ));

    lines.join("\n")
}

/// Format workspace report as JSON.
pub fn workspace_report_json(report: &WorkspaceReport) -> serde_json::Value {
    use serde_json::json;

    let projects: Vec<serde_json::Value> = report
        .results
        .iter()
        .map(|r| {
            let mut proj = json!({
                "path": r.project.path.display().to_string(),
                "language": r.project.language,
                "framework": r.project.framework,
                "duration_ms": r.duration.as_millis(),
                "skipped": r.skipped,
            });

            if let Some(ref error) = r.error {
                proj["error"] = json!(error);
            }

            if let Some(ref result) = r.result {
                proj["passed"] = json!(result.is_success());
                proj["total_tests"] = json!(result.total_tests());
                proj["total_passed"] = json!(result.total_passed());
                proj["total_failed"] = json!(result.total_failed());
            }

            proj
        })
        .collect();

    json!({
        "projects": projects,
        "projects_found": report.projects_found,
        "projects_run": report.projects_run,
        "projects_passed": report.projects_passed,
        "projects_failed": report.projects_failed,
        "projects_skipped": report.projects_skipped,
        "total_tests": report.total_tests,
        "total_passed": report.total_passed,
        "total_failed": report.total_failed,
        "total_duration_ms": report.total_duration.as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discover_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(projects.is_empty());
    }

    #[test]
    fn discover_single_rust_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].language, "Rust");
    }

    #[test]
    fn discover_multiple_projects() {
        let tmp = TempDir::new().unwrap();

        // Root Rust project
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        // Nested Go project
        let go_dir = tmp.path().join("services").join("api");
        fs::create_dir_all(&go_dir).unwrap();
        fs::write(go_dir.join("go.mod"), "module example.com/api\n").unwrap();
        fs::write(go_dir.join("main_test.go"), "package main\n").unwrap();

        // Nested Python project
        let py_dir = tmp.path().join("tools").join("scripts");
        fs::create_dir_all(&py_dir).unwrap();
        fs::write(py_dir.join("pyproject.toml"), "[tool.pytest]\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);

        assert!(
            projects.len() >= 3,
            "Expected at least 3 projects, found {}",
            projects.len()
        );

        let languages: Vec<&str> = projects.iter().map(|p| p.language.as_str()).collect();
        assert!(languages.contains(&"Rust"));
        assert!(languages.contains(&"Go"));
        assert!(languages.contains(&"Python"));
    }

    #[test]
    fn skip_node_modules() {
        let tmp = TempDir::new().unwrap();

        // Project in node_modules should be skipped
        let nm_dir = tmp.path().join("node_modules").join("some-package");
        fs::create_dir_all(&nm_dir).unwrap();
        fs::write(nm_dir.join("Cargo.toml"), "[package]\nname = \"inside\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(projects.is_empty());
    }

    #[test]
    fn skip_target_directory() {
        let tmp = TempDir::new().unwrap();

        // Root project
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        // target/ should be skipped
        let target_dir = tmp.path().join("target").join("debug").join("sub");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(
            target_dir.join("Cargo.toml"),
            "[package]\nname = \"target-inner\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1);
        // Compare using the original (non-canonicalized) path since discover_projects
        // stores dir.to_path_buf(). Canonicalize differs across platforms:
        // macOS: /var -> /private/var, Windows: short paths vs UNC paths.
        assert_eq!(projects[0].path, tmp.path().to_path_buf());
    }

    #[test]
    fn max_depth_limit() {
        let tmp = TempDir::new().unwrap();

        // Create deeply nested project
        let deep = tmp
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("f");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Cargo.toml"), "[package]\nname = \"deep\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            max_depth: 3,
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        // Should not find the deeply nested project
        assert!(projects.is_empty());
    }

    #[test]
    fn filter_by_language() {
        let tmp = TempDir::new().unwrap();

        // Rust project
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        // Python project
        let py_dir = tmp.path().join("py");
        fs::create_dir_all(&py_dir).unwrap();
        fs::write(py_dir.join("pyproject.toml"), "[tool.pytest]\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            filter_languages: vec!["rust".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].language, "Rust");
    }

    #[test]
    fn workspace_report_summary() {
        let report = WorkspaceReport {
            results: vec![],
            total_duration: Duration::from_secs(5),
            projects_found: 3,
            projects_run: 3,
            projects_passed: 2,
            projects_failed: 1,
            projects_skipped: 0,
            total_tests: 50,
            total_passed: 48,
            total_failed: 2,
        };

        let output = format_workspace_report(&report);
        assert!(output.contains("3 projects found"));
        assert!(output.contains("50 tests"));
    }

    #[test]
    fn workspace_report_json_format() {
        let report = WorkspaceReport {
            results: vec![],
            total_duration: Duration::from_secs(5),
            projects_found: 2,
            projects_run: 2,
            projects_passed: 1,
            projects_failed: 1,
            projects_skipped: 0,
            total_tests: 30,
            total_passed: 28,
            total_failed: 2,
        };

        let json = workspace_report_json(&report);
        assert_eq!(json["projects_found"], 2);
        assert_eq!(json["total_tests"], 30);
        assert_eq!(json["total_failed"], 2);
    }

    // ─── Effective jobs ───

    #[test]
    fn effective_jobs_auto() {
        let config = WorkspaceConfig::default();
        assert_eq!(config.max_jobs, 0);
        let jobs = config.effective_jobs();
        assert!(jobs >= 1, "auto-detected jobs should be >= 1, got {jobs}");
    }

    #[test]
    fn effective_jobs_explicit() {
        let config = WorkspaceConfig {
            max_jobs: 8,
            ..Default::default()
        };
        assert_eq!(config.effective_jobs(), 8);
    }

    // ─── Custom skip dirs ───

    #[test]
    fn custom_skip_dirs() {
        let tmp = TempDir::new().unwrap();

        // Project in "experiments" dir
        let exp_dir = tmp.path().join("experiments");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(exp_dir.join("Cargo.toml"), "[package]\nname = \"exp\"\n").unwrap();

        // Project in root
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            skip_dirs: vec!["experiments".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].language, "Rust");
    }

    // ─── Include dirs override ───

    #[test]
    fn include_dirs_overrides_default_skip() {
        let tmp = TempDir::new().unwrap();

        // Project inside "packages" dir (normally skipped by SKIP_DIRS)
        let pkg_dir = tmp.path().join("packages").join("shared-fixtures");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "shared-fixtures", "scripts": {"test": "jest"}}"#,
        )
        .unwrap();

        // Project in root
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();

        // Without include_dirs: packages/ is skipped
        let config_default = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config_default);
        assert_eq!(projects.len(), 1, "packages/ should be skipped by default");
        assert_eq!(projects[0].language, "Rust");

        // With include_dirs: packages/ is scanned
        let config_include = WorkspaceConfig {
            include_dirs: vec!["packages".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config_include);
        assert_eq!(
            projects.len(),
            2,
            "packages/ should be scanned when included"
        );
        let languages: Vec<&str> = projects.iter().map(|p| p.language.as_str()).collect();
        assert!(languages.contains(&"JavaScript"));
        assert!(languages.contains(&"Rust"));
    }

    #[test]
    fn include_dirs_does_not_affect_custom_skip() {
        let tmp = TempDir::new().unwrap();

        // Project in "experiments" dir
        let exp_dir = tmp.path().join("experiments");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(exp_dir.join("Cargo.toml"), "[package]\nname = \"exp\"\n").unwrap();

        let engine = DetectionEngine::new();

        // include_dirs only overrides SKIP_DIRS, not custom skip_dirs
        let config = WorkspaceConfig {
            skip_dirs: vec!["experiments".to_string()],
            include_dirs: vec!["packages".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 0, "custom skip_dirs should still apply");
    }

    // ─── Symlink loop protection ───

    #[cfg(unix)]
    #[test]
    fn symlink_loop_does_not_hang() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        // Create a symlink loop: sub/loop -> parent
        std::os::unix::fs::symlink(tmp.path(), sub.join("loop")).unwrap();

        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        // This should not hang or crash
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1);
    }

    // ─── build_report ───

    #[test]
    fn build_report_empty() {
        let report = build_report(vec![], 0, Duration::from_secs(0));
        assert_eq!(report.projects_found, 0);
        assert_eq!(report.projects_run, 0);
        assert_eq!(report.projects_passed, 0);
        assert_eq!(report.projects_failed, 0);
        assert_eq!(report.total_tests, 0);
    }

    #[test]
    fn build_report_with_results() {
        use crate::adapters::{TestCase, TestStatus, TestSuite};

        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/test"),
            language: "Rust".to_string(),
            framework: "cargo".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let results = vec![
            WorkspaceRunResult {
                project: project.clone(),
                result: Some(TestRunResult {
                    suites: vec![TestSuite {
                        name: "suite1".to_string(),
                        tests: vec![
                            TestCase {
                                name: "test_a".to_string(),
                                status: TestStatus::Passed,
                                duration: Duration::from_millis(10),
                                error: None,
                            },
                            TestCase {
                                name: "test_b".to_string(),
                                status: TestStatus::Passed,
                                duration: Duration::from_millis(20),
                                error: None,
                            },
                        ],
                    }],
                    raw_exit_code: 0,
                    duration: Duration::from_millis(30),
                }),
                duration: Duration::from_millis(50),
                error: None,
                skipped: false,
            },
            WorkspaceRunResult {
                project: project.clone(),
                result: None,
                duration: Duration::ZERO,
                error: None,
                skipped: true,
            },
        ];

        let report = build_report(results, 3, Duration::from_secs(1));
        assert_eq!(report.projects_found, 3);
        assert_eq!(report.projects_run, 1);
        assert_eq!(report.projects_passed, 1);
        assert_eq!(report.projects_failed, 0);
        assert_eq!(report.projects_skipped, 1);
        assert_eq!(report.total_tests, 2);
        assert_eq!(report.total_passed, 2);
        assert_eq!(report.total_failed, 0);
    }

    #[test]
    fn build_report_with_failures() {
        use crate::adapters::{TestCase, TestError, TestStatus, TestSuite};

        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/test"),
            language: "Go".to_string(),
            framework: "go test".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let results = vec![WorkspaceRunResult {
            project: project.clone(),
            result: Some(TestRunResult {
                suites: vec![TestSuite {
                    name: "suite".to_string(),
                    tests: vec![
                        TestCase {
                            name: "pass".to_string(),
                            status: TestStatus::Passed,
                            duration: Duration::from_millis(5),
                            error: None,
                        },
                        TestCase {
                            name: "fail".to_string(),
                            status: TestStatus::Failed,
                            duration: Duration::from_millis(5),
                            error: Some(TestError {
                                message: "expected true".to_string(),
                                location: None,
                            }),
                        },
                    ],
                }],
                raw_exit_code: 1,
                duration: Duration::from_millis(10),
            }),
            duration: Duration::from_millis(20),
            error: None,
            skipped: false,
        }];

        let report = build_report(results, 1, Duration::from_secs(1));
        assert_eq!(report.projects_failed, 1);
        assert_eq!(report.projects_passed, 0);
        assert_eq!(report.total_tests, 2);
        assert_eq!(report.total_passed, 1);
        assert_eq!(report.total_failed, 1);
    }

    #[test]
    fn build_report_error_counts_as_failed() {
        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/test"),
            language: "Rust".to_string(),
            framework: "cargo".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let results = vec![WorkspaceRunResult {
            project,
            result: None,
            duration: Duration::ZERO,
            error: Some("runner not found".to_string()),
            skipped: false,
        }];

        let report = build_report(results, 1, Duration::from_secs(0));
        assert_eq!(report.projects_failed, 1);
        assert_eq!(report.projects_passed, 0);
        assert_eq!(report.projects_run, 0); // error is not counted as "run"
    }

    // ─── Format report variants ───

    #[test]
    fn format_report_skipped_project() {
        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/myproj"),
            language: "Rust".to_string(),
            framework: "cargo".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let report = WorkspaceReport {
            results: vec![WorkspaceRunResult {
                project,
                result: None,
                duration: Duration::ZERO,
                error: None,
                skipped: true,
            }],
            total_duration: Duration::from_secs(0),
            projects_found: 1,
            projects_run: 0,
            projects_passed: 0,
            projects_failed: 0,
            projects_skipped: 1,
            total_tests: 0,
            total_passed: 0,
            total_failed: 0,
        };

        let output = format_workspace_report(&report);
        assert!(
            output.contains("skipped"),
            "should mention skipped: {output}"
        );
    }

    #[test]
    fn format_report_error_project() {
        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/badproj"),
            language: "Go".to_string(),
            framework: "go test".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let report = WorkspaceReport {
            results: vec![WorkspaceRunResult {
                project,
                result: None,
                duration: Duration::ZERO,
                error: Some("go not found".to_string()),
                skipped: false,
            }],
            total_duration: Duration::from_secs(0),
            projects_found: 1,
            projects_run: 0,
            projects_passed: 0,
            projects_failed: 1,
            projects_skipped: 0,
            total_tests: 0,
            total_passed: 0,
            total_failed: 0,
        };

        let output = format_workspace_report(&report);
        assert!(output.contains("error"), "should mention error: {output}");
        assert!(output.contains("go not found"));
    }

    // ─── JSON report with project details ───

    #[test]
    fn json_report_with_project_results() {
        use crate::adapters::{TestCase, TestStatus, TestSuite};

        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/proj"),
            language: "Python".to_string(),
            framework: "pytest".to_string(),
            confidence: 0.9,
            adapter_index: 0,
        };

        let report = WorkspaceReport {
            results: vec![WorkspaceRunResult {
                project,
                result: Some(TestRunResult {
                    suites: vec![TestSuite {
                        name: "suite".to_string(),
                        tests: vec![TestCase {
                            name: "test_x".to_string(),
                            status: TestStatus::Passed,
                            duration: Duration::from_millis(5),
                            error: None,
                        }],
                    }],
                    raw_exit_code: 0,
                    duration: Duration::from_millis(5),
                }),
                duration: Duration::from_millis(100),
                error: None,
                skipped: false,
            }],
            total_duration: Duration::from_secs(1),
            projects_found: 1,
            projects_run: 1,
            projects_passed: 1,
            projects_failed: 0,
            projects_skipped: 0,
            total_tests: 1,
            total_passed: 1,
            total_failed: 0,
        };

        let json = workspace_report_json(&report);
        assert_eq!(json["projects"][0]["language"], "Python");
        assert_eq!(json["projects"][0]["framework"], "pytest");
        assert_eq!(json["projects"][0]["passed"], true);
        assert_eq!(json["projects"][0]["total_tests"], 1);
        assert_eq!(json["projects"][0]["skipped"], false);
    }

    #[test]
    fn json_report_with_error_project() {
        let project = WorkspaceProject {
            path: PathBuf::from("/tmp/err"),
            language: "Rust".to_string(),
            framework: "cargo".to_string(),
            confidence: 1.0,
            adapter_index: 0,
        };

        let report = WorkspaceReport {
            results: vec![WorkspaceRunResult {
                project,
                result: None,
                duration: Duration::ZERO,
                error: Some("compilation failed".to_string()),
                skipped: false,
            }],
            total_duration: Duration::from_secs(0),
            projects_found: 1,
            projects_run: 0,
            projects_passed: 0,
            projects_failed: 1,
            projects_skipped: 0,
            total_tests: 0,
            total_passed: 0,
            total_failed: 0,
        };

        let json = workspace_report_json(&report);
        assert_eq!(json["projects"][0]["error"], "compilation failed");
    }

    // ─── Discovery edge cases ───

    #[test]
    fn discover_respects_depth_zero_unlimited() {
        let tmp = TempDir::new().unwrap();
        let deep = tmp
            .path()
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("f")
            .join("g");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Cargo.toml"), "[package]\nname = \"deep\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            max_depth: 0, // unlimited
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1, "depth=0 should be unlimited");
    }

    #[test]
    fn discover_multiple_languages_sorted_by_path() {
        let tmp = TempDir::new().unwrap();

        // Create projects in reverse alphabetical order
        let z_dir = tmp.path().join("z-project");
        fs::create_dir_all(&z_dir).unwrap();
        fs::write(z_dir.join("Cargo.toml"), "[package]\nname = \"z\"\n").unwrap();

        let a_dir = tmp.path().join("a-project");
        fs::create_dir_all(&a_dir).unwrap();
        fs::write(a_dir.join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(projects.len() >= 2);
        // Should be sorted by path
        for w in projects.windows(2) {
            assert!(w[0].path <= w[1].path, "projects should be sorted by path");
        }
    }

    #[test]
    fn filter_languages_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            filter_languages: vec!["RUST".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 1, "filter should be case-insensitive");
    }

    #[test]
    fn filter_no_match() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            filter_languages: vec!["java".to_string()],
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(
            projects.is_empty(),
            "should find no Rust projects when filtering for Java"
        );
    }

    // ─── Config defaults ───

    #[test]
    fn workspace_config_defaults() {
        let config = WorkspaceConfig::default();
        assert_eq!(config.max_depth, 5);
        assert!(config.parallel);
        assert_eq!(config.max_jobs, 0);
        assert!(!config.fail_fast);
        assert!(config.filter_languages.is_empty());
        assert!(config.skip_dirs.is_empty());
    }

    // ─── Recursion depth safety ───

    #[test]
    fn deep_recursion_100_levels_respects_depth_limit() {
        let tmp = TempDir::new().unwrap();

        // Create a 100-level deep directory tree
        let mut current = tmp.path().to_path_buf();
        for i in 0..100 {
            current = current.join(format!("level_{}", i));
        }
        fs::create_dir_all(&current).unwrap();
        fs::write(
            current.join("Cargo.toml"),
            "[package]\nname = \"deep100\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            max_depth: 5,
            ..Default::default()
        };
        // Should NOT find the deeply nested project and should NOT stack overflow
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(
            projects.is_empty(),
            "should not discover project at depth 100 with max_depth=5"
        );
    }

    #[test]
    fn deep_recursion_unlimited_depth_handles_deep_trees() {
        let tmp = TempDir::new().unwrap();

        // Create a 50-level deep directory tree with a project at the bottom
        let mut current = tmp.path().to_path_buf();
        for i in 0..50 {
            current = current.join(format!("d{}", i));
        }
        fs::create_dir_all(&current).unwrap();
        fs::write(current.join("Cargo.toml"), "[package]\nname = \"deep50\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            max_depth: 0, // unlimited
            ..Default::default()
        };
        // Should find it without stack overflow
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(
            projects.len(),
            1,
            "should find deep project with unlimited depth"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_chain_does_not_hang() {
        let tmp = TempDir::new().unwrap();
        // A -> B -> C -> A (multi-hop symlink loop)
        let dir_a = tmp.path().join("a");
        let dir_b = tmp.path().join("b");
        let dir_c = tmp.path().join("c");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();
        fs::create_dir_all(&dir_c).unwrap();
        std::os::unix::fs::symlink(&dir_b, dir_a.join("link_to_b")).unwrap();
        std::os::unix::fs::symlink(&dir_c, dir_b.join("link_to_c")).unwrap();
        std::os::unix::fs::symlink(&dir_a, dir_c.join("link_to_a")).unwrap();

        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"root\"\n",
        )
        .unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        // Must complete without hanging
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert!(
            !projects.is_empty(),
            "should find at least the root project"
        );
    }

    #[cfg(unix)]
    #[test]
    fn self_referencing_symlink_safe() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        // Symlink pointing to itself
        std::os::unix::fs::symlink(&sub, sub.join("self")).unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        // Should complete without infinite loop
        assert!(projects.is_empty());
    }

    // ─── Memory growth safety ───

    #[test]
    fn broad_directory_tree_no_excessive_memory() {
        let tmp = TempDir::new().unwrap();

        // Create 500 sibling directories (wide tree)
        for i in 0..500 {
            let dir = tmp.path().join(format!("project_{}", i));
            fs::create_dir_all(&dir).unwrap();
            // Each directory is just empty — no project marker
        }

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let projects = discover_projects(tmp.path(), &engine, &config);
        // Should handle 500 directories without issues
        assert!(projects.is_empty(), "empty dirs should produce no projects");
    }

    #[test]
    fn many_projects_discovered_without_crash() {
        let tmp = TempDir::new().unwrap();

        // Create 50 Rust projects
        for i in 0..50 {
            let dir = tmp.path().join(format!("proj_{}", i));
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("Cargo.toml"),
                format!("[package]\nname = \"proj_{}\"\n", i),
            )
            .unwrap();
        }

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig {
            max_depth: 2,
            ..Default::default()
        };
        let projects = discover_projects(tmp.path(), &engine, &config);
        assert_eq!(projects.len(), 50, "should discover all 50 projects");
    }

    #[test]
    fn visited_set_prevents_re_scanning() {
        // This ensures visited HashSet actually prevents revisiting
        let tmp = TempDir::new().unwrap();

        // Create two paths to the same directory
        let real_dir = tmp.path().join("real");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(real_dir.join("Cargo.toml"), "[package]\nname = \"real\"\n").unwrap();

        let engine = DetectionEngine::new();
        let config = WorkspaceConfig::default();
        let mut projects = Vec::new();
        let mut visited = HashSet::new();
        let skip_set: HashSet<&str> = SKIP_DIRS.iter().copied().collect();
        let custom_skip: HashSet<String> = HashSet::new();

        // Scan same directory twice
        scan_dir(
            &real_dir,
            &engine,
            &config,
            &skip_set,
            &custom_skip,
            0,
            &mut projects,
            &mut visited,
        );
        scan_dir(
            &real_dir,
            &engine,
            &config,
            &skip_set,
            &custom_skip,
            0,
            &mut projects,
            &mut visited,
        );

        // Should only appear once due to visited set
        assert_eq!(
            projects.len(),
            1,
            "visited set should prevent duplicate scanning"
        );
    }
}
