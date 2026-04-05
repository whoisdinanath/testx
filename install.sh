#!/bin/sh
# testx installer — downloads the appropriate prebuilt binary.
# Usage: curl -fsSL https://raw.githubusercontent.com/whoisdinanath/testx/main/install.sh | sh

set -eu

REPO="whoisdinanath/testx"
INSTALL_DIR="${TESTX_INSTALL_DIR:-${HOME}/.local/bin}"

info() { printf '\033[0;34m%s\033[0m\n' "$*"; }
error() { printf '\033[0;31merror: %s\033[0m\n' "$*" >&2; exit 1; }

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      error "Unsupported OS: $os. Install from source: cargo install testx-cli" ;;
    esac

    case "$arch" in
        x86_64|amd64)   arch="x86_64" ;;
        aarch64|arm64)  arch="aarch64" ;;
        *)              error "Unsupported architecture: $arch" ;;
    esac

    echo "${os}-${arch}"
}

get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/'
    else
        error "curl or wget required"
    fi
}

download() {
    local url="$1" dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    fi
}

main() {
    local version="${TESTX_VERSION:-}"
    if [ -z "$version" ]; then
        info "Fetching latest release..."
        version="$(get_latest_version)"
        if [ -z "$version" ]; then
            error "Could not determine latest version. Set TESTX_VERSION=x.y.z and retry."
        fi
    fi

    local platform
    platform="$(detect_platform)"

    local archive_name="testx-v${version}-${platform}.tar.gz"
    local url="https://github.com/${REPO}/releases/download/v${version}/${archive_name}"

    info "Downloading testx v${version} for ${platform}..."

    local tmpdir
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    download "$url" "${tmpdir}/${archive_name}"

    tar xzf "${tmpdir}/${archive_name}" -C "$tmpdir"

    mkdir -p "$INSTALL_DIR"
    mv "${tmpdir}/testx" "${INSTALL_DIR}/testx"
    chmod +x "${INSTALL_DIR}/testx"

    info "Installed testx v${version} to ${INSTALL_DIR}/testx"

    # Check if INSTALL_DIR is on PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            echo "Add the following to your shell config:"
            echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
            ;;
    esac

    echo ""
    info "Run 'testx --help' to get started."
}

main "$@"
