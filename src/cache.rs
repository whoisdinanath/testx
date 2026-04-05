use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::TestRunResult;
use crate::error::{Result, TestxError};
use crate::hash::StableHasher;

/// Directory name for the cache store.
const CACHE_DIR: &str = ".testx";
const CACHE_FILE: &str = "cache.json";
const MAX_CACHE_ENTRIES: usize = 100;

/// Configuration for smart caching.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled.
    pub enabled: bool,
    /// Maximum age of cache entries in seconds.
    pub max_age_secs: u64,
    /// Maximum number of cache entries.
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age_secs: 86400, // 24 hours
            max_entries: MAX_CACHE_ENTRIES,
        }
    }
}

/// A cached test result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    /// Content hash of the project files.
    pub hash: String,
    /// Adapter name used.
    pub adapter: String,
    /// Timestamp when cached (epoch seconds).
    pub timestamp: u64,
    /// Whether the cached run passed.
    pub passed: bool,
    /// Number of tests that passed / failed / skipped.
    pub total_passed: usize,
    pub total_failed: usize,
    pub total_skipped: usize,
    pub total_tests: usize,
    /// Duration of the original run in milliseconds.
    pub duration_ms: u64,
    /// Extra args used.
    pub extra_args: Vec<String>,
}

impl CacheEntry {
    pub fn is_expired(&self, max_age_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp) > max_age_secs
    }

    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp)
    }
}

/// The persistent cache store.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheStore {
    pub entries: Vec<CacheEntry>,
}

impl CacheStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Load cache from disk.
    pub fn load(project_dir: &Path) -> Self {
        let cache_path = project_dir.join(CACHE_DIR).join(CACHE_FILE);
        if !cache_path.exists() {
            return Self::new();
        }

        match std::fs::read_to_string(&cache_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Self::new()),
            Err(_) => Self::new(),
        }
    }

    /// Save cache to disk.
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let cache_dir = project_dir.join(CACHE_DIR);

        // Guard against symlink-based cache poisoning: if .testx is a symlink,
        // refuse to write through it.
        if cache_dir.exists() && cache_dir.read_link().is_ok() {
            return Err(TestxError::IoError {
                context: format!(
                    "Cache directory is a symlink (possible symlink attack): {}",
                    cache_dir.display()
                ),
                source: std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "symlink in cache path",
                ),
            });
        }

        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).map_err(|e| TestxError::IoError {
                context: format!("Failed to create cache directory: {}", cache_dir.display()),
                source: e,
            })?;
        }

        let cache_path = cache_dir.join(CACHE_FILE);

        // Also check if the cache file itself is a symlink
        if cache_path.exists() && cache_path.read_link().is_ok() {
            return Err(TestxError::IoError {
                context: format!(
                    "Cache file is a symlink (possible symlink attack): {}",
                    cache_path.display()
                ),
                source: std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "symlink in cache path",
                ),
            });
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| TestxError::ConfigError {
            message: format!("Failed to serialize cache: {}", e),
        })?;

        std::fs::write(&cache_path, content).map_err(|e| TestxError::IoError {
            context: format!("Failed to write cache file: {}", cache_path.display()),
            source: e,
        })?;

        Ok(())
    }

    /// Look up a cache entry by hash.
    pub fn lookup(&self, hash: &str, config: &CacheConfig) -> Option<&CacheEntry> {
        self.entries
            .iter()
            .rev() // Most recent first
            .find(|e| e.hash == hash && !e.is_expired(config.max_age_secs))
    }

    /// Insert a new cache entry.
    pub fn insert(&mut self, entry: CacheEntry, config: &CacheConfig) {
        // Remove old entries with the same hash
        self.entries.retain(|e| e.hash != entry.hash);
        self.entries.push(entry);
        self.prune(config);
    }

    /// Remove expired and excess entries.
    pub fn prune(&mut self, config: &CacheConfig) {
        // Remove expired entries
        self.entries.retain(|e| !e.is_expired(config.max_age_secs));

        // Keep only the most recent entries
        if self.entries.len() > config.max_entries {
            let excess = self.entries.len() - config.max_entries;
            self.entries.drain(..excess);
        }
    }

    /// Clear all cache entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for CacheStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a content hash of the project's source files.
///
/// This walks the project directory, collecting file modification times and sizes
/// for relevant source files, then produces a combined hash.
pub fn compute_project_hash(project_dir: &Path, adapter_name: &str) -> Result<String> {
    let mut hasher = StableHasher::new();

    // Hash the adapter name so different adapters have different cache keys
    adapter_name.hash(&mut hasher);

    // Collect relevant files and their metadata
    let mut file_entries: Vec<(String, u64, u64)> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    collect_source_files(project_dir, project_dir, &mut file_entries, 0, &mut visited)?;

    // Sort for determinism
    file_entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (path, mtime, size) in &file_entries {
        path.hash(&mut hasher);
        mtime.hash(&mut hasher);
        size.hash(&mut hasher);
    }

    let hash = hasher.finish();
    Ok(format!("{:016x}", hash))
}

/// Maximum recursion depth for source file collection.
const MAX_SOURCE_DEPTH: usize = 20;

/// Recursively collect source files with their modification time and size.
fn collect_source_files(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<(String, u64, u64)>,
    depth: usize,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
) -> Result<()> {
    if depth > MAX_SOURCE_DEPTH {
        return Ok(());
    }

    // Canonicalize to avoid symlink loops
    if let Ok(canonical) = dir.canonicalize()
        && !visited.insert(canonical)
    {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(dir).map_err(|e| TestxError::IoError {
        context: format!("Failed to read directory: {}", dir.display()),
        source: e,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| TestxError::IoError {
            context: "Failed to read directory entry".into(),
            source: e,
        })?;

        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Skip hidden directories, build artifacts, and caches
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "__pycache__"
            || name == "build"
            || name == "dist"
            || name == "vendor"
            || name == ".testx"
        {
            continue;
        }

        let file_type = entry.file_type().map_err(|e| TestxError::IoError {
            context: format!("Failed to get file type: {}", path.display()),
            source: e,
        })?;

        if file_type.is_dir() {
            collect_source_files(root, &path, entries, depth + 1, visited)?;
        } else if file_type.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && is_source_extension(ext)
        {
            let metadata = std::fs::metadata(&path).map_err(|e| TestxError::IoError {
                context: format!("Failed to read metadata: {}", path.display()),
                source: e,
            })?;

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            entries.push((rel_path, mtime, metadata.len()));
        }
    }

    Ok(())
}

/// Check if a file extension indicates a source file.
fn is_source_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "go"
            | "py"
            | "pyi"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "java"
            | "kt"
            | "kts"
            | "cpp"
            | "cc"
            | "cxx"
            | "c"
            | "h"
            | "hpp"
            | "hxx"
            | "rb"
            | "ex"
            | "exs"
            | "php"
            | "cs"
            | "fs"
            | "vb"
            | "zig"
            | "toml"
            | "json"
            | "xml"
            | "yaml"
            | "yml"
            | "cfg"
            | "ini"
            | "gradle"
            | "properties"
            | "cmake"
            | "lock"
            | "mod"
            | "sum"
    )
}

/// Cache a test run result.
pub fn cache_result(
    project_dir: &Path,
    hash: &str,
    adapter: &str,
    result: &TestRunResult,
    extra_args: &[String],
    config: &CacheConfig,
) -> Result<()> {
    let mut store = CacheStore::load(project_dir);

    let entry = CacheEntry {
        hash: hash.to_string(),
        adapter: adapter.to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        passed: result.is_success(),
        total_passed: result.total_passed(),
        total_failed: result.total_failed(),
        total_skipped: result.total_skipped(),
        total_tests: result.total_tests(),
        duration_ms: result.duration.as_millis() as u64,
        extra_args: extra_args.to_vec(),
    };

    store.insert(entry, config);
    store.save(project_dir)
}

/// Check if we have a cached result.
pub fn check_cache(project_dir: &Path, hash: &str, config: &CacheConfig) -> Option<CacheEntry> {
    let store = CacheStore::load(project_dir);
    store.lookup(hash, config).cloned()
}

/// Format cache hit info for display.
pub fn format_cache_hit(entry: &CacheEntry) -> String {
    let age = entry.age_secs();
    let age_str = if age < 60 {
        format!("{}s ago", age)
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else {
        format!("{}h ago", age / 3600)
    };

    format!(
        "Cache hit ({}) — {} tests: {} passed, {} failed, {} skipped ({:.1}ms, cached {})",
        entry.adapter,
        entry.total_tests,
        entry.total_passed,
        entry.total_failed,
        entry.total_skipped,
        entry.duration_ms as f64,
        age_str,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{TestCase, TestStatus, TestSuite};
    use std::time::Duration;

    fn make_result() -> TestRunResult {
        TestRunResult {
            suites: vec![TestSuite {
                name: "suite".to_string(),
                tests: vec![
                    TestCase {
                        name: "test_1".to_string(),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(10),
                        error: None,
                    },
                    TestCase {
                        name: "test_2".to_string(),
                        status: TestStatus::Passed,
                        duration: Duration::from_millis(20),
                        error: None,
                    },
                ],
            }],
            duration: Duration::from_millis(30),
            raw_exit_code: 0,
        }
    }

    #[test]
    fn cache_store_new_empty() {
        let store = CacheStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn cache_store_insert_and_lookup() {
        let config = CacheConfig::default();
        let mut store = CacheStore::new();

        let entry = CacheEntry {
            hash: "abc123".to_string(),
            adapter: "Rust".to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            passed: true,
            total_passed: 5,
            total_failed: 0,
            total_skipped: 1,
            total_tests: 6,
            duration_ms: 123,
            extra_args: vec![],
        };

        store.insert(entry.clone(), &config);

        assert_eq!(store.len(), 1);
        let found = store.lookup("abc123", &config);
        assert!(found.is_some());
        assert_eq!(found.unwrap().adapter, "Rust");
    }

    #[test]
    fn cache_store_lookup_miss() {
        let config = CacheConfig::default();
        let store = CacheStore::new();
        assert!(store.lookup("nonexistent", &config).is_none());
    }

    #[test]
    fn cache_store_replaces_same_hash() {
        let config = CacheConfig::default();
        let mut store = CacheStore::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry1 = CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now,
            passed: true,
            total_passed: 5,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 5,
            duration_ms: 100,
            extra_args: vec![],
        };

        let entry2 = CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now + 1,
            passed: false,
            total_passed: 3,
            total_failed: 2,
            total_skipped: 0,
            total_tests: 5,
            duration_ms: 200,
            extra_args: vec![],
        };

        store.insert(entry1, &config);
        store.insert(entry2, &config);

        assert_eq!(store.len(), 1);
        let found = store.lookup("abc", &config).unwrap();
        assert!(!found.passed);
        assert_eq!(found.total_failed, 2);
    }

    #[test]
    fn cache_entry_expiry() {
        let entry = CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: 0, // Very old
            passed: true,
            total_passed: 5,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 5,
            duration_ms: 100,
            extra_args: vec![],
        };

        assert!(entry.is_expired(86400));
    }

    #[test]
    fn cache_entry_not_expired() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now,
            passed: true,
            total_passed: 5,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 5,
            duration_ms: 100,
            extra_args: vec![],
        };

        assert!(!entry.is_expired(86400));
    }

    #[test]
    fn cache_store_prune_expired() {
        let config = CacheConfig {
            max_age_secs: 10,
            ..Default::default()
        };

        let mut store = CacheStore::new();

        // Old entry
        store.entries.push(CacheEntry {
            hash: "old".to_string(),
            adapter: "Rust".to_string(),
            timestamp: 0,
            passed: true,
            total_passed: 1,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 1,
            duration_ms: 10,
            extra_args: vec![],
        });

        // Fresh entry
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        store.entries.push(CacheEntry {
            hash: "new".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now,
            passed: true,
            total_passed: 1,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 1,
            duration_ms: 10,
            extra_args: vec![],
        });

        store.prune(&config);
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].hash, "new");
    }

    #[test]
    fn cache_store_prune_excess() {
        let config = CacheConfig {
            max_entries: 2,
            ..Default::default()
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut store = CacheStore::new();
        for i in 0..5 {
            store.entries.push(CacheEntry {
                hash: format!("hash_{}", i),
                adapter: "Rust".to_string(),
                timestamp: now,
                passed: true,
                total_passed: 1,
                total_failed: 0,
                total_skipped: 0,
                total_tests: 1,
                duration_ms: 10,
                extra_args: vec![],
            });
        }

        store.prune(&config);
        assert_eq!(store.len(), 2);
        // Should keep the most recent (last two)
        assert_eq!(store.entries[0].hash, "hash_3");
        assert_eq!(store.entries[1].hash, "hash_4");
    }

    #[test]
    fn cache_store_clear() {
        let mut store = CacheStore::new();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        store.entries.push(CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now,
            passed: true,
            total_passed: 1,
            total_failed: 0,
            total_skipped: 0,
            total_tests: 1,
            duration_ms: 10,
            extra_args: vec![],
        });

        assert!(!store.is_empty());
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn cache_store_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut store = CacheStore::new();
        store.entries.push(CacheEntry {
            hash: "disk_test".to_string(),
            adapter: "Go".to_string(),
            timestamp: now,
            passed: true,
            total_passed: 3,
            total_failed: 0,
            total_skipped: 1,
            total_tests: 4,
            duration_ms: 500,
            extra_args: vec!["-v".to_string()],
        });

        store.save(dir.path()).unwrap();

        let loaded = CacheStore::load(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries[0].hash, "disk_test");
        assert_eq!(loaded.entries[0].adapter, "Go");
        assert_eq!(loaded.entries[0].total_passed, 3);
        assert_eq!(loaded.entries[0].extra_args, vec!["-v"]);
    }

    #[test]
    fn cache_store_load_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = CacheStore::load(dir.path());
        assert!(store.is_empty());
    }

    #[test]
    fn cache_store_load_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join(CACHE_DIR);
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join(CACHE_FILE), "not valid json").unwrap();

        let store = CacheStore::load(dir.path());
        assert!(store.is_empty());
    }

    #[test]
    fn compute_hash_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let hash1 = compute_project_hash(dir.path(), "Rust").unwrap();
        let hash2 = compute_project_hash(dir.path(), "Rust").unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn compute_hash_different_adapters() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let hash_rust = compute_project_hash(dir.path(), "Rust").unwrap();
        let hash_go = compute_project_hash(dir.path(), "Go").unwrap();
        assert_ne!(hash_rust, hash_go);
    }

    #[test]
    fn compute_hash_changes_with_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let hash1 = compute_project_hash(dir.path(), "Rust").unwrap();

        // Modify file (changes mtime and/or size)
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(
            dir.path().join("main.rs"),
            "fn main() { println!(\"hello\"); }",
        )
        .unwrap();

        let hash2 = compute_project_hash(dir.path(), "Rust").unwrap();
        // Hash should change because mtime/size changed
        // Note: on some filesystems mtime resolution may be coarse, so we also check size
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn compute_hash_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let hash = compute_project_hash(dir.path(), "Rust").unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn compute_hash_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join("config"), "git stuff").unwrap();

        // Adding files to .git shouldn't change the hash
        let hash1 = compute_project_hash(dir.path(), "Rust").unwrap();
        std::fs::write(dir.path().join(".git").join("newfile"), "more stuff").unwrap();
        let hash2 = compute_project_hash(dir.path(), "Rust").unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn is_source_ext_coverage() {
        assert!(is_source_extension("rs"));
        assert!(is_source_extension("py"));
        assert!(is_source_extension("js"));
        assert!(is_source_extension("go"));
        assert!(is_source_extension("java"));
        assert!(is_source_extension("cpp"));

        assert!(!is_source_extension("md"));
        assert!(!is_source_extension("png"));
        assert!(!is_source_extension("txt"));
        assert!(!is_source_extension(""));
    }

    #[test]
    fn format_cache_hit_display() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = CacheEntry {
            hash: "abc".to_string(),
            adapter: "Rust".to_string(),
            timestamp: now - 120, // 2 minutes ago
            passed: true,
            total_passed: 10,
            total_failed: 0,
            total_skipped: 2,
            total_tests: 12,
            duration_ms: 1500,
            extra_args: vec![],
        };

        let output = format_cache_hit(&entry);
        assert!(output.contains("Rust"));
        assert!(output.contains("12 tests"));
        assert!(output.contains("10 passed"));
        assert!(output.contains("2m ago"));
    }

    #[test]
    fn cache_result_and_check() {
        let dir = tempfile::tempdir().unwrap();
        let config = CacheConfig::default();
        let result = make_result();

        cache_result(dir.path(), "test_hash", "Rust", &result, &[], &config).unwrap();

        let cached = check_cache(dir.path(), "test_hash", &config);
        assert!(cached.is_some());
        let entry = cached.unwrap();
        assert!(entry.passed);
        assert_eq!(entry.total_tests, 2);
        assert_eq!(entry.total_passed, 2);
    }

    #[test]
    fn cache_miss_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        let config = CacheConfig::default();
        let result = make_result();

        cache_result(dir.path(), "hash_a", "Rust", &result, &[], &config).unwrap();

        let cached = check_cache(dir.path(), "hash_b", &config);
        assert!(cached.is_none());
    }

    #[test]
    fn cache_config_defaults() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_age_secs, 86400);
        assert_eq!(config.max_entries, 100);
    }

    // ─── Recursion depth safety ───

    #[test]
    fn collect_source_files_deep_nesting_no_crash() {
        let dir = tempfile::tempdir().unwrap();

        // Create a 50-level deep directory tree with source files
        let mut current = dir.path().to_path_buf();
        for i in 0..50 {
            current = current.join(format!("level_{}", i));
        }
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("deep.rs"), "fn deep() {}").unwrap();

        // Should not crash or stack overflow
        let hash = compute_project_hash(dir.path(), "Rust").unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn collect_source_files_respects_max_depth() {
        let dir = tempfile::tempdir().unwrap();

        // Create a tree deeper than MAX_SOURCE_DEPTH (20)
        let mut current = dir.path().to_path_buf();
        for i in 0..25 {
            current = current.join(format!("d{}", i));
        }
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("too_deep.rs"), "fn too_deep() {}").unwrap();

        // The file at depth 25 should be unreachable
        let mut entries = Vec::new();
        let mut visited = std::collections::HashSet::new();
        collect_source_files(dir.path(), dir.path(), &mut entries, 0, &mut visited).unwrap();

        // No file entry should contain "too_deep" since it's past MAX_SOURCE_DEPTH
        assert!(
            !entries.iter().any(|(path, _, _)| path.contains("too_deep")),
            "files beyond MAX_SOURCE_DEPTH should not be collected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn collect_source_files_symlink_loop_safe() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("lib.rs"), "fn lib() {}").unwrap();

        // Create symlink loop: src/loop -> parent
        std::os::unix::fs::symlink(dir.path(), sub.join("loop")).unwrap();

        // Should not hang
        let hash = compute_project_hash(dir.path(), "Rust").unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn collect_source_files_many_files_no_crash() {
        let dir = tempfile::tempdir().unwrap();

        // Create 200 source files in one directory
        for i in 0..200 {
            std::fs::write(
                dir.path().join(format!("file_{}.rs", i)),
                format!("fn f{}() {{}}", i),
            )
            .unwrap();
        }

        let hash = compute_project_hash(dir.path(), "Rust").unwrap();
        assert!(!hash.is_empty());

        // Verify all files were collected
        let mut entries = Vec::new();
        let mut visited = std::collections::HashSet::new();
        collect_source_files(dir.path(), dir.path(), &mut entries, 0, &mut visited).unwrap();
        assert_eq!(entries.len(), 200, "should collect all 200 source files");
    }
}
