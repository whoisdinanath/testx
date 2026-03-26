//! Built-in reporter plugins for testx.
//!
//! Each reporter generates output in a specific format:
//! - **Markdown**: Human-readable Markdown report
//! - **GitHub**: GitHub Actions annotations
//! - **HTML**: Self-contained HTML report
//! - **Notify**: Desktop notifications

pub mod github;
pub mod html;
pub mod markdown;
pub mod notify;
