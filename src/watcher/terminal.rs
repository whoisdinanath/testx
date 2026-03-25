use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// Actions the user can perform in watch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    /// Re-run all tests.
    RunAll,
    /// Re-run only failed tests.
    RunFailed,
    /// Quit watch mode.
    Quit,
    /// No action (continue waiting).
    Continue,
    /// Clear screen and re-run.
    ClearAndRun,
}

/// Non-blocking keypresses reader for watch mode's interactive terminal.
pub struct TerminalInput {
    rx: Receiver<u8>,
    _handle: thread::JoinHandle<()>,
}

impl TerminalInput {
    /// Start reading stdin in a background thread.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let stdin = io::stdin();
            let mut buf = [0u8; 1];
            loop {
                match stdin.lock().read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(buf[0]).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            rx,
            _handle: handle,
        }
    }

    /// Poll for a keypress (non-blocking).
    pub fn poll(&self) -> WatchAction {
        match self.rx.try_recv() {
            Ok(key) => Self::key_to_action(key),
            Err(TryRecvError::Empty) => WatchAction::Continue,
            Err(TryRecvError::Disconnected) => WatchAction::Quit,
        }
    }

    /// Convert a keypress to an action.
    fn key_to_action(key: u8) -> WatchAction {
        match key {
            b'q' | b'Q' => WatchAction::Quit,
            b'a' | b'A' => WatchAction::RunAll,
            b'f' | b'F' => WatchAction::RunFailed,
            b'c' | b'C' => WatchAction::ClearAndRun,
            b'\n' | b'\r' => WatchAction::RunAll,
            _ => WatchAction::Continue,
        }
    }
}

impl Default for TerminalInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Clear the terminal screen.
pub fn clear_screen() {
    // ANSI escape: clear entire screen and move cursor to top-left
    print!("\x1B[2J\x1B[1;1H");
}

/// Print the watch mode status bar.
pub fn print_watch_status(changed_count: usize) {
    use colored::Colorize;

    println!();
    println!(
        "  {} {}",
        "watching".cyan().bold(),
        format!("{} file(s) changed", changed_count).dimmed(),
    );
    println!(
        "  {} {}",
        "keys:".dimmed(),
        "a = run all · f = run failed · q = quit · Enter = re-run".dimmed()
    );
    println!();
}

/// Print a separator line for watch mode re-runs.
pub fn print_watch_separator() {
    use colored::Colorize;

    println!();
    println!(
        "{}",
        "════════════════════════════════════════════════════════════"
            .cyan()
            .dimmed()
    );
    println!();
}

/// Print watch mode startup message.
pub fn print_watch_start(root: &std::path::Path) {
    use colored::Colorize;

    println!();
    println!(
        "  {} {} {}",
        "testx".bold().cyan(),
        "watch mode".bold(),
        format!("({})", root.display()).dimmed(),
    );
    println!(
        "  {} {}",
        "keys:".dimmed(),
        "a = run all · f = run failed · q = quit · Enter = re-run".dimmed()
    );
    println!("{}", "─".repeat(60).dimmed());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_to_action_quit() {
        assert_eq!(TerminalInput::key_to_action(b'q'), WatchAction::Quit);
        assert_eq!(TerminalInput::key_to_action(b'Q'), WatchAction::Quit);
    }

    #[test]
    fn key_to_action_run_all() {
        assert_eq!(TerminalInput::key_to_action(b'a'), WatchAction::RunAll);
        assert_eq!(TerminalInput::key_to_action(b'A'), WatchAction::RunAll);
        assert_eq!(TerminalInput::key_to_action(b'\n'), WatchAction::RunAll);
        assert_eq!(TerminalInput::key_to_action(b'\r'), WatchAction::RunAll);
    }

    #[test]
    fn key_to_action_run_failed() {
        assert_eq!(
            TerminalInput::key_to_action(b'f'),
            WatchAction::RunFailed
        );
        assert_eq!(
            TerminalInput::key_to_action(b'F'),
            WatchAction::RunFailed
        );
    }

    #[test]
    fn key_to_action_clear() {
        assert_eq!(
            TerminalInput::key_to_action(b'c'),
            WatchAction::ClearAndRun
        );
    }

    #[test]
    fn key_to_action_unknown() {
        assert_eq!(
            TerminalInput::key_to_action(b'x'),
            WatchAction::Continue
        );
        assert_eq!(
            TerminalInput::key_to_action(b'z'),
            WatchAction::Continue
        );
    }

    #[test]
    fn watch_action_equality() {
        assert_eq!(WatchAction::Quit, WatchAction::Quit);
        assert_ne!(WatchAction::Quit, WatchAction::RunAll);
    }

    #[test]
    fn clear_screen_does_not_panic() {
        // This just tests that the function doesn't crash
        // (output goes to stdout which is fine in tests)
        clear_screen();
    }
}
