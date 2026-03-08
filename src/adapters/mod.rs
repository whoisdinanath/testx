use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

pub mod cpp;
pub mod dotnet;
pub mod elixir;
pub mod go;
pub mod java;
pub mod javascript;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod util;
pub mod zig;

/// Status of a single test case
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

/// A single test case result
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestCase {
    pub name: String,
    pub status: TestStatus,
    #[serde(serialize_with = "serialize_duration_ms")]
    pub duration: Duration,
    /// Error message + location if failed
    pub error: Option<TestError>,
}

/// Error details for a failed test
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TestError {
    pub message: String,
    pub location: Option<String>,
}

/// A group of test cases (typically a file or class)
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
}

impl TestSuite {
    pub fn passed(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.status == TestStatus::Passed)
            .count()
    }

    pub fn failed(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .count()
    }

    /// Returns all failed test cases with their error details
    pub fn failures(&self) -> Vec<&TestCase> {
        self.tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .collect()
    }

    pub fn is_passed(&self) -> bool {
        self.failed() == 0
    }
}

/// Complete result of a test run
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestRunResult {
    pub suites: Vec<TestSuite>,
    #[serde(serialize_with = "serialize_duration_ms")]
    pub duration: Duration,
    pub raw_exit_code: i32,
}

impl TestRunResult {
    pub fn total_passed(&self) -> usize {
        self.suites.iter().map(|s| s.passed()).sum()
    }

    pub fn total_failed(&self) -> usize {
        self.suites.iter().map(|s| s.failed()).sum()
    }

    pub fn total_skipped(&self) -> usize {
        self.suites.iter().map(|s| s.skipped()).sum()
    }

    pub fn total_tests(&self) -> usize {
        self.suites.iter().map(|s| s.tests.len()).sum()
    }

    pub fn is_success(&self) -> bool {
        self.total_failed() == 0
    }

    /// Get all tests sorted by duration (slowest first)
    pub fn slowest_tests(&self, n: usize) -> Vec<(&TestSuite, &TestCase)> {
        let mut all: Vec<_> = self
            .suites
            .iter()
            .flat_map(|s| s.tests.iter().map(move |t| (s, t)))
            .collect();
        all.sort_by(|a, b| b.1.duration.cmp(&a.1.duration));
        all.into_iter().take(n).collect()
    }
}

/// What was detected about a project
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub language: String,
    pub framework: String,
    pub confidence: f32,
}

/// Serialize a Duration as milliseconds (f64) for clean JSON output.
fn serialize_duration_ms<S>(d: &Duration, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_f64(d.as_secs_f64() * 1000.0)
}

/// Trait that each language adapter must implement
pub trait TestAdapter {
    /// Check if this adapter can handle the project at the given path
    fn detect(&self, project_dir: &Path) -> Option<DetectionResult>;

    /// Build the command to run tests
    fn build_command(&self, project_dir: &Path, extra_args: &[String]) -> Result<Command>;

    /// Parse stdout/stderr from the test runner into structured results
    fn parse_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> TestRunResult;

    /// Name of this adapter for display
    fn name(&self) -> &str;

    /// Check if the required test runner binary is available on PATH
    fn check_runner(&self) -> Option<String> {
        None // Default: no check
    }
}
