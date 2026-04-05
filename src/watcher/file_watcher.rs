use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};

use crate::config::WatchConfig;
use crate::watcher::debouncer::Debouncer;
use crate::watcher::glob::{GlobPattern, should_ignore};

/// Watcher backend: native OS events or polling fallback.
/// Variants hold the watcher to keep it alive for the duration of `FileWatcher`.
#[allow(dead_code)]
enum WatcherBackend {
    Native(notify::RecommendedWatcher),
    Poll(notify::PollWatcher),
}

/// File system watcher that detects changes in a project directory.
pub struct FileWatcher {
    /// Receiver for file change events from the watcher backend.
    rx: Receiver<PathBuf>,
    /// Debouncer to coalesce rapid changes.
    debouncer: Debouncer,
    /// Glob patterns for paths to ignore.
    ignore_patterns: Vec<GlobPattern>,
    /// Root directory being watched.
    root: PathBuf,
    /// Watcher backend kept alive for the lifetime of `FileWatcher`.
    _backend: WatcherBackend,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory.
    ///
    /// Uses native OS filesystem events by default (inotify on Linux,
    /// kqueue on macOS, ReadDirectoryChanges on Windows). Falls back to
    /// polling when `config.poll_ms` is set — useful for network
    /// filesystems that don't support native events.
    pub fn new(root: &Path, config: &WatchConfig) -> std::io::Result<Self> {
        let ignore_patterns: Vec<GlobPattern> =
            config.ignore.iter().map(|p| GlobPattern::new(p)).collect();

        let debouncer = Debouncer::new(config.debounce_ms);
        let (tx, rx) = mpsc::channel();

        // Event handler: extract paths from notify events into the channel.
        let handler = move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send(path);
                }
            }
        };

        let backend = if let Some(poll_ms) = config.poll_ms {
            let poll_config =
                notify::Config::default().with_poll_interval(Duration::from_millis(poll_ms));
            let mut w =
                notify::PollWatcher::new(handler, poll_config).map_err(std::io::Error::other)?;
            w.watch(root, RecursiveMode::Recursive)
                .map_err(std::io::Error::other)?;
            WatcherBackend::Poll(w)
        } else {
            let mut w = notify::recommended_watcher(handler).map_err(std::io::Error::other)?;
            w.watch(root, RecursiveMode::Recursive)
                .map_err(std::io::Error::other)?;
            WatcherBackend::Native(w)
        };

        Ok(Self {
            rx,
            debouncer,
            ignore_patterns,
            root: root.to_path_buf(),
            _backend: backend,
        })
    }

    /// Wait for file changes and return the changed paths (debounced).
    /// This blocks until changes are detected.
    pub fn wait_for_changes(&mut self) -> Vec<PathBuf> {
        loop {
            // Drain pending events
            if self.drain_pending() {
                // Watcher disconnected, return what we have
                if self.debouncer.has_pending() {
                    return self.debouncer.flush();
                }
                return Vec::new();
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

    /// Non-blocking poll for file changes with a timeout.
    ///
    /// Returns changed paths (debounced) or an empty vec if no changes
    /// occurred within the timeout. Does NOT block indefinitely.
    pub fn poll_changes(&mut self, timeout: Duration) -> Vec<PathBuf> {
        let deadline = std::time::Instant::now() + timeout;

        loop {
            // Drain pending events
            if self.drain_pending() {
                if self.debouncer.has_pending() {
                    return self.debouncer.flush();
                }
                return Vec::new();
            }

            // Check if we should flush
            if self.debouncer.should_flush() {
                return self.debouncer.flush();
            }

            if self.debouncer.has_pending() {
                let remaining = self.debouncer.time_remaining();
                if remaining > Duration::ZERO {
                    std::thread::sleep(remaining.min(Duration::from_millis(50)));
                    continue;
                }
                return self.debouncer.flush();
            }

            // Check if we've exceeded the timeout
            let now = std::time::Instant::now();
            if now >= deadline {
                return Vec::new();
            }

            let wait = (deadline - now).min(Duration::from_millis(100));
            match self.rx.recv_timeout(wait) {
                Ok(path) => {
                    if !self.should_ignore(&path) {
                        self.debouncer.add(path);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Vec::new(),
            }
        }
    }

    /// Drain all pending events from the channel.
    /// Returns true if the channel is disconnected.
    fn drain_pending(&mut self) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(path) => {
                    if !self.should_ignore(&path) {
                        self.debouncer.add(path);
                    }
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_watcher_construction() {
        let dir = tempfile::tempdir().unwrap();
        let config = WatchConfig::default();
        let watcher = FileWatcher::new(dir.path(), &config);
        assert!(watcher.is_ok());
        let watcher = watcher.unwrap();
        assert_eq!(watcher.root(), dir.path());
    }

    #[test]
    fn file_watcher_poll_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let config = WatchConfig {
            poll_ms: Some(200),
            ..WatchConfig::default()
        };
        let watcher = FileWatcher::new(dir.path(), &config);
        assert!(watcher.is_ok());
    }

    #[test]
    fn file_watcher_ignore_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let config = WatchConfig {
            ignore: vec!["*.log".to_string(), "target/**".to_string()],
            ..WatchConfig::default()
        };
        let watcher = FileWatcher::new(dir.path(), &config).unwrap();
        assert!(watcher.should_ignore(&dir.path().join("build.log")));
        assert!(!watcher.should_ignore(&dir.path().join("main.rs")));
    }

    #[test]
    fn file_watcher_nonexistent_dir() {
        let config = WatchConfig::default();
        let result = FileWatcher::new(Path::new("/nonexistent/dir"), &config);
        assert!(result.is_err());
    }
}
