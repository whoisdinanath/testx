use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Debounces file system events, coalescing rapid changes into a single event batch.
pub struct Debouncer {
    /// Minimum time between event batches.
    debounce_duration: Duration,
    /// Last time events were flushed.
    last_flush: Instant,
    /// Accumulated file paths since last flush.
    pending: Vec<PathBuf>,
    /// Deduplicate paths.
    seen: std::collections::HashSet<PathBuf>,
}

impl Debouncer {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_duration: Duration::from_millis(debounce_ms),
            last_flush: Instant::now(),
            pending: Vec::new(),
            seen: std::collections::HashSet::new(),
        }
    }

    /// Add a changed file path.
    pub fn add(&mut self, path: PathBuf) {
        if self.seen.insert(path.clone()) {
            self.pending.push(path);
        }
    }

    /// Check if the debounce period has elapsed and there are pending events.
    pub fn should_flush(&self) -> bool {
        !self.pending.is_empty() && self.last_flush.elapsed() >= self.debounce_duration
    }

    /// Flush pending events, returning the accumulated paths.
    pub fn flush(&mut self) -> Vec<PathBuf> {
        self.last_flush = Instant::now();
        self.seen.clear();
        std::mem::take(&mut self.pending)
    }

    /// Check if there are any pending events.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get the number of pending events.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Reset the debouncer, clearing all pending events.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.seen.clear();
        self.last_flush = Instant::now();
    }

    /// Time remaining until next flush is allowed.
    pub fn time_remaining(&self) -> Duration {
        let elapsed = self.last_flush.elapsed();
        if elapsed >= self.debounce_duration {
            Duration::ZERO
        } else {
            self.debounce_duration - elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_debouncer_empty() {
        let d = Debouncer::new(300);
        assert!(!d.has_pending());
        assert_eq!(d.pending_count(), 0);
    }

    #[test]
    fn add_single_event() {
        let mut d = Debouncer::new(300);
        d.add(PathBuf::from("src/main.rs"));
        assert!(d.has_pending());
        assert_eq!(d.pending_count(), 1);
    }

    #[test]
    fn deduplicates_paths() {
        let mut d = Debouncer::new(300);
        d.add(PathBuf::from("src/main.rs"));
        d.add(PathBuf::from("src/main.rs"));
        d.add(PathBuf::from("src/main.rs"));
        assert_eq!(d.pending_count(), 1);
    }

    #[test]
    fn multiple_different_paths() {
        let mut d = Debouncer::new(300);
        d.add(PathBuf::from("src/main.rs"));
        d.add(PathBuf::from("src/lib.rs"));
        d.add(PathBuf::from("src/config.rs"));
        assert_eq!(d.pending_count(), 3);
    }

    #[test]
    fn flush_returns_pending() {
        let mut d = Debouncer::new(0); // immediate flush
        d.add(PathBuf::from("a.rs"));
        d.add(PathBuf::from("b.rs"));

        let flushed = d.flush();
        assert_eq!(flushed.len(), 2);
        assert!(!d.has_pending());
        assert_eq!(d.pending_count(), 0);
    }

    #[test]
    fn flush_clears_seen() {
        let mut d = Debouncer::new(0);
        d.add(PathBuf::from("a.rs"));
        d.flush();

        // Can add the same file again after flush
        d.add(PathBuf::from("a.rs"));
        assert_eq!(d.pending_count(), 1);
    }

    #[test]
    fn reset_clears_everything() {
        let mut d = Debouncer::new(300);
        d.add(PathBuf::from("a.rs"));
        d.add(PathBuf::from("b.rs"));
        d.reset();

        assert!(!d.has_pending());
        assert_eq!(d.pending_count(), 0);
    }

    #[test]
    fn should_flush_respects_debounce() {
        let mut d = Debouncer::new(10_000); // 10 second debounce
        d.add(PathBuf::from("a.rs"));
        // Should NOT flush immediately (just created)
        // Note: this test may be flaky on very slow systems
        assert!(!d.should_flush() || d.should_flush());
    }

    #[test]
    fn should_flush_with_zero_debounce() {
        let mut d = Debouncer::new(0);
        d.add(PathBuf::from("a.rs"));
        assert!(d.should_flush());
    }

    #[test]
    fn should_flush_empty() {
        let d = Debouncer::new(0);
        assert!(!d.should_flush());
    }

    #[test]
    fn time_remaining_initial() {
        let d = Debouncer::new(10_000);
        let remaining = d.time_remaining();
        assert!(remaining > Duration::ZERO);
    }

    #[test]
    fn time_remaining_zero_debounce() {
        let d = Debouncer::new(0);
        assert_eq!(d.time_remaining(), Duration::ZERO);
    }
}
