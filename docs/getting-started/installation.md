# Installation

## From source (recommended)

Requires [Rust](https://rustup.rs/) 1.87 or newer.

```bash
cargo install --path .
```

Or build from git:

```bash
git clone https://github.com/whoisdinanath/testx.git
cd testx
cargo build --release
# Binary is at target/release/testx
```

## From releases

Download a prebuilt binary from the [releases page](https://github.com/whoisdinanath/testx/releases) for your platform:

| Platform               | Target                      |
| ---------------------- | --------------------------- |
| Linux (x86_64)         | `x86_64-unknown-linux-gnu`  |
| Linux (x86_64, static) | `x86_64-unknown-linux-musl` |
| Linux (ARM64)          | `aarch64-unknown-linux-gnu` |
| macOS (Intel)          | `x86_64-apple-darwin`       |
| macOS (Apple Silicon)  | `aarch64-apple-darwin`      |
| Windows                | `x86_64-pc-windows-msvc`    |

## Shell completions

```bash
# Bash
testx completions bash > ~/.local/share/bash-completion/completions/testx

# Zsh
testx completions zsh > ~/.local/share/zsh/site-functions/_testx

# Fish
testx completions fish > ~/.config/fish/completions/testx.fish

# PowerShell
testx completions powershell >> $PROFILE
```

## Verify installation

```bash
testx --version
testx list   # Show supported frameworks
```
