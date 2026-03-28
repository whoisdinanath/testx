use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use crate::config::WatchConfig;
use crate::watcher::debouncer::Debouncer;
use crate::watcher::glob::{GlobPattern, should_ignore};

/// File system watcher that detects changes in a project directory.
pub struct FileWatcher {
    /// Receiver for file change events from the watcher thread.
    rx: Receiver<PathBuf>,
    /// Debouncer to coalesce rapid changes.
    debouncer: Debouncer,
    /// Glob patterns for paths to ignore.
    ignore_patterns: Vec<GlobPattern>,
    /// Root directory being watched.
    root: PathBuf,
    /// Handle to the watcher thread.
    _watcher_handle: std::thread::JoinHandle<()>,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory.
    pub fn new(root: &Path, config: &WatchConfig) -> std::io::Result<Self> {
        let ignore_patterns: Vec<GlobPattern> =
            config.ignore.iter().map(|p| GlobPattern::new(p)).collect();

        let debouncer = Debouncer::new(config.debounce_ms);
        let (tx, rx) = mpsc::channel();

        let poll_interval = config
            .poll_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(500));

        let watch_root = root.to_path_buf();

        // Spawn a polling watcher thread
        let watcher_handle = std::thread::spawn(move || {
            let mut known_files = collect_files(&watch_root);
            let mut known_mtimes = get_mtimes(&known_files);

            loop {
                std::thread::sleep(poll_interval);

                let current_files = collect_files(&watch_root);
                let current_mtimes = get_mtimes(&current_files);

                // Find new or modified files
                for (path, mtime) in &current_mtimes {
                    let changed = match known_mtimes.get(path) {
                        Some(old_mtime) => mtime != old_mtime,
                        None => true, // new file
                    };

                    if changed && tx.send(path.clone()).is_err() {
                        return; // receiver dropped, stop watching
                    }
                }

                // Find deleted files (send parent dir)
                for path in &known_files {
                    if !current_files.contains(path)
                        && let Some(parent) = path.parent()
                        && tx.send(parent.to_path_buf()).is_err()
                    {
                        return;
                    }
                }

                known_files = current_files;
                known_mtimes = current_mtimes;
            }
        });

        Ok(Self {
            rx,
            debouncer,
            ignore_patterns,
            root: root.to_path_buf(),
            _watcher_handle: watcher_handle,
        })
    }

    /// Wait for file changes and return the changed paths (debounced).
    /// This blocks until changes are detected.
    pub fn wait_for_changes(&mut self) -> Vec<PathBuf> {
        loop {
            // Drain pending events
            loop {
                match self.rx.try_recv() {
                    Ok(path) => {
                        if !self.should_ignore(&path) {
                            self.debouncer.add(path);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        // Watcher thread died, return pending
                        if self.debouncer.has_pending() {
                            return self.debouncer.flush();
                        }
                        return Vec::new();
                    }
                }
            }

            // Check if we should flush
            if self.debouncer.should_flush() {
                return self.debouncer.flush();
            }

            // Wait a bit for more events
            if self.debouncer.has_pending() {
                let remaining = self.debouncer.time_remaining();
                if remaining > Duration::ZERO {
                    std::thread::sleep(remaining.min(Duration::from_millis(50)));
                    continue;
                }
                return self.debouncer.flush();
            }

            // Block waiting for next event
            match self.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(path) => {
                    if !self.should_ignore(&path) {
                        self.debouncer.add(path);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Vec::new(),
            }
        }
    }

    /// Check if a path should be ignored.
    fn should_ignore(&self, path: &Path) -> bool {
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy();

        should_ignore(&relative, &self.ignore_patterns)
    }

    /// Get the root directory being watched.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Recursively collect all files in a directory.
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(dir, &mut files);
    files
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden directories and common ignores for performance
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "__pycache__"
                || name == ".testx")
        {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// Get modification times for a list of files.
fn get_mtimes(files: &[PathBuf]) -> std::collections::HashMap<PathBuf, std::time::SystemTime> {
    let mut mtimes = std::collections::HashMap::new();
    for file in files {
        if let Ok(meta) = std::fs::metadata(file)
            && let Ok(mtime) = meta.modified()
        {
            mtimes.insert(file.clone(), mtime);
        }
    }
    mtimes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_files_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn test() {}").unwrap();

        let files = collect_files(dir.path());
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_files_skips_hidden() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), "cfg").unwrap();
        std::fs::write(dir.path().join("main.rs"), "main").unwrap();

        let files = collect_files(dir.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap() == "main.rs");
    }

    #[test]
    fn collect_files_recursive_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "lib").unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "main").unwrap();

        let files = collect_files(dir.path());
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_files(dir.path());
        assert!(files.is_empty());
    }

    #[test]
    fn get_mtimes_for_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "a").unwrap();
        std::fs::write(dir.path().join("b.rs"), "b").unwrap();

        let files = collect_files(dir.path());
        let mtimes = get_mtimes(&files);
        assert_eq!(mtimes.len(), 2);
    }

    #[test]
    fn get_mtimes_nonexistent_files() {
        let files = vec![PathBuf::from("/nonexistent/file.rs")];
        let mtimes = get_mtimes(&files);
        assert!(mtimes.is_empty());
    }

    #[test]
    fn file_watcher_construction() {
        let dir = tempfile::tempdir().unwrap();
        let config = WatchConfig::default();
        let watcher = FileWatcher::new(dir.path(), &config);
        assert!(watcher.is_ok());
        let watcher = watcher.unwrap();
        assert_eq!(watcher.root(), dir.path());
    }
}
