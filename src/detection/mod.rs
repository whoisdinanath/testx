use std::path::Path;

use crate::adapters::cpp::CppAdapter;
use crate::adapters::dotnet::DotnetAdapter;
use crate::adapters::elixir::ElixirAdapter;
use crate::adapters::go::GoAdapter;
use crate::adapters::java::JavaAdapter;
use crate::adapters::javascript::JavaScriptAdapter;
use crate::adapters::php::PhpAdapter;
use crate::adapters::python::PythonAdapter;
use crate::adapters::ruby::RubyAdapter;
use crate::adapters::rust::RustAdapter;
use crate::adapters::zig::ZigAdapter;
use crate::adapters::{DetectionResult, TestAdapter};

pub struct DetectionEngine {
    adapters: Vec<Box<dyn TestAdapter>>,
}

pub struct DetectedProject {
    pub detection: DetectionResult,
    pub adapter_index: usize,
}

impl Default for DetectionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DetectionEngine {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(RustAdapter::new()),
                Box::new(GoAdapter::new()),
                Box::new(PythonAdapter::new()),
                Box::new(JavaScriptAdapter::new()),
                Box::new(JavaAdapter::new()),
                Box::new(CppAdapter::new()),
                Box::new(RubyAdapter::new()),
                Box::new(ElixirAdapter::new()),
                Box::new(PhpAdapter::new()),
                Box::new(DotnetAdapter::new()),
                Box::new(ZigAdapter::new()),
            ],
        }
    }

    /// Detect the best matching test framework for the given project directory.
    /// Returns the detection result and a reference to the matching adapter.
    pub fn detect(&self, project_dir: &Path) -> Option<DetectedProject> {
        let mut best: Option<DetectedProject> = None;

        for (i, adapter) in self.adapters.iter().enumerate() {
            if let Some(result) = adapter.detect(project_dir) {
                let dominated = best
                    .as_ref()
                    .map(|b| result.confidence > b.detection.confidence)
                    .unwrap_or(true);
                if dominated {
                    best = Some(DetectedProject {
                        detection: result,
                        adapter_index: i,
                    });
                    // Early exit on very high confidence — no need to scan remaining adapters
                    if best
                        .as_ref()
                        .is_some_and(|b| b.detection.confidence >= 0.95)
                    {
                        break;
                    }
                }
            }
        }

        best
    }

    /// Detect all matching frameworks (for polyglot projects).
    pub fn detect_all(&self, project_dir: &Path) -> Vec<DetectedProject> {
        let mut results = Vec::new();
        for (i, adapter) in self.adapters.iter().enumerate() {
            if let Some(result) = adapter.detect(project_dir) {
                results.push(DetectedProject {
                    detection: result,
                    adapter_index: i,
                });
            }
        }
        results.sort_by(|a, b| {
            b.detection
                .confidence
                .partial_cmp(&a.detection.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Get an adapter by index.
    pub fn adapter(&self, index: usize) -> &dyn TestAdapter {
        self.adapters[index].as_ref()
    }

    /// Get all registered adapters.
    pub fn adapters(&self) -> &[Box<dyn TestAdapter>] {
        &self.adapters
    }

    /// Number of built-in adapters (registered at construction time).
    /// Custom adapters are appended after these.
    pub const BUILTIN_COUNT: usize = 11;

    /// Register a custom adapter. Custom adapters are appended after built-in
    /// ones and participate in normal confidence-based detection.
    pub fn register(&mut self, adapter: Box<dyn TestAdapter>) {
        self.adapters.push(adapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let engine = DetectionEngine::new();
        let det = engine.detect(dir.path()).unwrap();
        assert_eq!(det.detection.language, "Rust");
    }

    #[test]
    fn detect_go_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example.com/test\n").unwrap();
        std::fs::write(dir.path().join("main_test.go"), "package main\n").unwrap();
        let engine = DetectionEngine::new();
        let det = engine.detect(dir.path()).unwrap();
        assert_eq!(det.detection.language, "Go");
    }

    #[test]
    fn detect_python_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]\n").unwrap();
        let engine = DetectionEngine::new();
        let det = engine.detect(dir.path()).unwrap();
        assert_eq!(det.detection.language, "Python");
    }

    #[test]
    fn detect_js_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"jest":"^29"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("jest.config.js"), "").unwrap();
        let engine = DetectionEngine::new();
        let det = engine.detect(dir.path()).unwrap();
        assert_eq!(det.detection.language, "JavaScript");
    }

    #[test]
    fn detect_nothing_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let engine = DetectionEngine::new();
        assert!(engine.detect(dir.path()).is_none());
    }

    #[test]
    fn detect_all_polyglot() {
        let dir = tempfile::tempdir().unwrap();
        // Both Rust and Python
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]\n").unwrap();
        let engine = DetectionEngine::new();
        let all = engine.detect_all(dir.path());
        assert!(all.len() >= 2);
    }

    #[test]
    fn adapter_count() {
        let engine = DetectionEngine::new();
        assert_eq!(engine.adapters().len(), 11);
    }
}
