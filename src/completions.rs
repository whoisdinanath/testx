//! Shell completion generation.
//!
//! Generates shell completions for bash, zsh, fish, and PowerShell.
//!
//! # Usage
//!
//! ```sh
//! testx completions bash > ~/.local/share/bash-completion/completions/testx
//! testx completions zsh > ~/.zfunc/_testx
//! testx completions fish > ~/.config/fish/completions/testx.fish
//! testx completions powershell > _testx.ps1
//! ```

use std::io;

use clap::Command;
use clap_complete::{Shell, generate};

/// Generate completions for the given shell and write to stdout.
pub fn generate_completions(shell: Shell, cmd: &mut Command) {
    generate(shell, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

/// Get the list of supported shells.
pub fn supported_shells() -> &'static [Shell] {
    &[Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell]
}

/// Get the recommended install path for each shell.
pub fn install_hint(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "testx completions bash > ~/.local/share/bash-completion/completions/testx",
        Shell::Zsh => "testx completions zsh > ~/.zfunc/_testx && echo 'fpath+=~/.zfunc' >> ~/.zshrc",
        Shell::Fish => "testx completions fish > ~/.config/fish/completions/testx.fish",
        Shell::PowerShell => "testx completions powershell > _testx.ps1",
        _ => "testx completions <shell> > <output-file>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command as ClapCommand};

    fn test_cmd() -> ClapCommand {
        ClapCommand::new("testx")
            .version("1.0.0")
            .about("Universal test runner")
            .arg(Arg::new("path").short('p').long("path"))
    }

    #[test]
    fn supported_shells_non_empty() {
        assert!(!supported_shells().is_empty());
        assert!(supported_shells().contains(&Shell::Bash));
        assert!(supported_shells().contains(&Shell::Zsh));
    }

    #[test]
    fn install_hint_bash() {
        let hint = install_hint(Shell::Bash);
        assert!(hint.contains("bash-completion"));
    }

    #[test]
    fn install_hint_zsh() {
        let hint = install_hint(Shell::Zsh);
        assert!(hint.contains("zfunc"));
    }

    #[test]
    fn install_hint_fish() {
        let hint = install_hint(Shell::Fish);
        assert!(hint.contains("fish/completions"));
    }

    #[test]
    fn install_hint_powershell() {
        let hint = install_hint(Shell::PowerShell);
        assert!(hint.contains("ps1"));
    }

    #[test]
    fn generate_bash_completions() {
        let mut cmd = test_cmd();
        // Just verify it doesn't panic
        let mut buf = Vec::new();
        generate(Shell::Bash, &mut cmd, "testx", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("testx"));
    }

    #[test]
    fn generate_zsh_completions() {
        let mut cmd = test_cmd();
        let mut buf = Vec::new();
        generate(Shell::Zsh, &mut cmd, "testx", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("testx"));
    }

    #[test]
    fn generate_fish_completions() {
        let mut cmd = test_cmd();
        let mut buf = Vec::new();
        generate(Shell::Fish, &mut cmd, "testx", &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("testx"));
    }
}
