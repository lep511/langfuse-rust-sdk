#!/bin/sh
# Install the `langfuse` CLI (langfuse-rust-cli) from its prebuilt static
# Linux binaries (langfuse-cli/dist/ of github.com/lep511/langfuse-rust-sdk).
#
#   curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh -s -- --dir ~/bin
#
# Options:
#   --dir DIR   Install directory [default: /usr/local/bin if writable, else ~/.local/bin]
#   -h, --help  Show this help
#
# Environment:
#   LANGFUSE_INSTALL_DIR    Same as --dir
#   LANGFUSE_CLI_BASE_URL   Where the binaries and SHA256SUMS are
#                           [default: the dist/ folder on the main branch]
#
# The download is verified against SHA256SUMS before it is installed.
# Uninstall: rm "$(command -v langfuse)"
set -eu

DEFAULT_BASE_URL="https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/dist"

say() {
    printf '%s\n' "$*"
}

err() {
    printf 'install.sh: error: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'USAGE'
Install the langfuse CLI (langfuse-rust-cli) from its prebuilt Linux binaries.

Usage: curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh -s -- [OPTIONS]

Options:
  --dir DIR   Install directory [default: /usr/local/bin if writable, else ~/.local/bin]
  -h, --help  Show this help

Environment:
  LANGFUSE_INSTALL_DIR    Same as --dir
  LANGFUSE_CLI_BASE_URL   Where the binaries and SHA256SUMS are [default: dist/ on main]
USAGE
}

download() {
    # download URL FILE
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https,file' --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        err "curl or wget is required"
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | sed 's/^.*= *//'
    else
        err "sha256sum, shasum or openssl is required to verify the download"
    fi
}

main() {
    dir="${LANGFUSE_INSTALL_DIR:-}"
    while [ $# -gt 0 ]; do
        case "$1" in
            --dir)
                [ $# -ge 2 ] || err "--dir requires a directory"
                dir="$2"
                shift 2
                ;;
            --dir=*)
                dir="${1#--dir=}"
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            *)
                err "unknown option: $1 (see --help)"
                ;;
        esac
    done

    os="$(uname -s)"
    [ "$os" = "Linux" ] || err "prebuilt binaries exist for Linux only (this is $os); build from source instead: https://github.com/lep511/langfuse-rust-sdk/tree/main/langfuse-cli#from-source"
    case "$(uname -m)" in
        x86_64 | amd64) arch="x86_64" ;;
        aarch64 | arm64) arch="aarch64" ;;
        *) err "no prebuilt binary for $(uname -m); build from source instead: https://github.com/lep511/langfuse-rust-sdk/tree/main/langfuse-cli#from-source" ;;
    esac
    asset="langfuse-${arch}-unknown-linux-musl"

    if [ -z "$dir" ]; then
        if [ -w /usr/local/bin ]; then
            dir="/usr/local/bin"
        else
            dir="${HOME:?HOME is not set}/.local/bin"
        fi
    fi
    mkdir -p "$dir" 2>/dev/null || true
    [ -d "$dir" ] && [ -w "$dir" ] || err "cannot write to $dir; choose another directory with --dir, or run the installer with sudo: curl -fsSL <url>/install.sh | sudo sh"

    base="${LANGFUSE_CLI_BASE_URL:-$DEFAULT_BASE_URL}"
    tmp="$(mktemp -d 2>/dev/null || mktemp -d -t langfuse)"
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 130' INT TERM

    say "Downloading $asset"
    download "$base/SHA256SUMS" "$tmp/SHA256SUMS" || err "cannot download $base/SHA256SUMS"
    download "$base/$asset" "$tmp/langfuse" || err "cannot download $base/$asset"

    expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$tmp/SHA256SUMS")"
    [ -n "$expected" ] || err "$asset is not listed in SHA256SUMS"
    actual="$(sha256 "$tmp/langfuse")"
    [ "$actual" = "$expected" ] || err "checksum mismatch for $asset (expected $expected, got $actual)"

    chmod 0755 "$tmp/langfuse"
    "$tmp/langfuse" --version >/dev/null 2>&1 || err "the downloaded binary does not run on this machine"
    # Copy, then rename: replaces a running binary safely.
    cp "$tmp/langfuse" "$dir/.langfuse.new"
    mv -f "$dir/.langfuse.new" "$dir/langfuse"

    say "Installed $("$dir/langfuse" --version) to $dir/langfuse"
    case ":${PATH:-}:" in
        *":$dir:"*)
            found="$(command -v langfuse 2>/dev/null || true)"
            if [ -n "$found" ] && [ "$found" != "$dir/langfuse" ]; then
                say "Note: $found comes first in PATH (another langfuse, e.g. the npm @langfuse/cli)."
            fi
            ;;
        *)
            say "Note: $dir is not in PATH. Add it, e.g.:"
            say "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile && . ~/.profile"
            ;;
    esac
    say "Set LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY and LANGFUSE_BASE_URL, then try: langfuse api health get"
}

main "$@"
