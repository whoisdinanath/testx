# Installation

There are several ways to install testx. Pick whichever method works best for your setup.

## Prerequisites

testx is a single binary with no runtime dependencies. You don't need to install anything else — just download or install the binary for your platform.

---

## npm (easiest for most developers)

If you already have [Node.js](https://nodejs.org/) installed (v16 or newer), this is the quickest way:

```bash
npm install -g @whoisdinanath/testx
```

This downloads a prebuilt native binary for your platform — no compilation needed. It works on macOS, Linux, and Windows (x64 and ARM64).

After installing, verify it works:

```bash
testx --version
```

---

## Install script (macOS / Linux)

A one-line installer that downloads the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/whoisdinanath/testx/main/install.sh | sh
```

By default, the binary is installed to `~/.local/bin`. If that directory isn't on your `PATH`, the script will tell you what to add to your shell config.

**Customization options:**

```bash
# Install a specific version
TESTX_VERSION=0.2.0 curl -fsSL https://raw.githubusercontent.com/whoisdinanath/testx/main/install.sh | sh

# Install to a custom directory
TESTX_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/whoisdinanath/testx/main/install.sh | sh
```

---

## From crates.io (Rust users)

If you have [Rust](https://rustup.rs/) installed (1.87 or newer):

```bash
cargo install testx-cli
```

This compiles from source, so it takes a minute or two. The binary is installed to `~/.cargo/bin/testx`.

---

## From GitHub releases (manual download)

Download a prebuilt binary from the [releases page](https://github.com/whoisdinanath/testx/releases).

Choose the right file for your system:

| Platform               | File to download                    |
| ---------------------- | ----------------------------------- |
| Linux (x86_64)         | `testx-v*-linux-x86_64.tar.gz`      |
| Linux (x86_64, static) | `testx-v*-linux-x86_64-musl.tar.gz` |
| Linux (ARM64)          | `testx-v*-linux-aarch64.tar.gz`     |
| macOS (Intel)          | `testx-v*-macos-x86_64.tar.gz`      |
| macOS (Apple Silicon)  | `testx-v*-macos-aarch64.tar.gz`     |
| Windows (x86_64)       | `testx-v*-windows-x86_64.zip`       |

**Linux / macOS:**

```bash
# Download and extract (example for Linux x86_64)
curl -LO https://github.com/whoisdinanath/testx/releases/latest/download/testx-v0.2.0-linux-x86_64.tar.gz
tar xzf testx-v0.2.0-linux-x86_64.tar.gz

# Move to a directory on your PATH
sudo mv testx /usr/local/bin/
```

**Windows:**

1. Download the `.zip` file from the releases page
2. Extract `testx.exe`
3. Move it to a directory on your `PATH` (e.g., `C:\Users\<you>\bin\`)

---

## From source

If you want to build from the latest code:

```bash
git clone https://github.com/whoisdinanath/testx.git
cd testx
cargo build --release
```

The binary will be at `target/release/testx`. Copy it somewhere on your `PATH`:

```bash
# Linux / macOS
sudo cp target/release/testx /usr/local/bin/

# Or install directly
cargo install --path .
```

---

## Shell completions (optional)

testx can generate tab-completion scripts for your shell. This enables auto-completing commands, flags, and options when you press Tab.

=== "Bash"
`bash
    # Add to your ~/.bashrc or run once:
    mkdir -p ~/.local/share/bash-completion/completions
    testx completions bash > ~/.local/share/bash-completion/completions/testx
    `

=== "Zsh"
`bash
    # Add to your ~/.zshrc or run once:
    mkdir -p ~/.local/share/zsh/site-functions
    testx completions zsh > ~/.local/share/zsh/site-functions/_testx
    `

=== "Fish"
`bash
    testx completions fish > ~/.config/fish/completions/testx.fish
    `

=== "PowerShell"
`powershell
    testx completions powershell >> $PROFILE
    `

Restart your shell (or `source` your config file) to activate completions.

---

## Verify installation

After installing, make sure everything works:

```bash
# Check the version
testx --version

# List all supported frameworks
testx list

# Try auto-detecting your project (run from a project directory)
testx detect
```

If `testx --version` prints a version number, you're good to go. Head to the [Quick Start](quickstart.md) guide next.

---

## Troubleshooting

**"command not found: testx"**
The binary isn't on your `PATH`. Check where it was installed and add that directory to your `PATH` environment variable.

**npm install fails with permissions error**
Try `npm install -g @whoisdinanath/testx --unsafe-perm` or use a Node version manager like [nvm](https://github.com/nvm-sh/nvm) to avoid needing `sudo`.

**Build from source fails**
Make sure you have Rust 1.87+ installed. Run `rustup update` to get the latest version.
