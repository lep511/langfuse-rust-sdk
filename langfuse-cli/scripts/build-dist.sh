#!/usr/bin/env bash
# Build static `langfuse` binaries and copy them to langfuse-cli/dist/.
#
# Usage: scripts/build-dist.sh [TARGET...]
#   Default targets: aarch64-unknown-linux-musl x86_64-unknown-linux-musl
#
# The binaries are cross-compiled with cargo-zigbuild, which uses zig as the
# C compiler and linker: `ring` (rustls' cryptography) has C and assembly
# parts, and musl builds need a musl C toolchain. Requirements:
#   rustup, zig (https://ziglang.org/download/), cargo install cargo-zigbuild
#
# Output in dist/: langfuse-<target> (statically linked), SHA256SUMS and
# VERSION. Paths of this machine (home, rustup, cargo, the checkout) are
# remapped so they do not end up in the binaries.
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="$crate_dir/dist"
default_targets=(aarch64-unknown-linux-musl x86_64-unknown-linux-musl)

usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac
targets=("$@")
if [ ${#targets[@]} -eq 0 ]; then
    targets=("${default_targets[@]}")
fi

fail() {
    echo "error: $*" >&2
    exit 1
}

command -v cargo >/dev/null || fail "cargo not found (install Rust with rustup)"
command -v rustup >/dev/null || fail "rustup not found (install Rust with rustup)"
command -v zig >/dev/null || fail "zig not found: install it from https://ziglang.org/download/ and put it on PATH"
cargo zigbuild --help >/dev/null 2>&1 || fail "cargo-zigbuild not found: cargo install cargo-zigbuild --locked"

cd "$crate_dir"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
sdk_version="$(awk '/^name = "langfuse-rust-sdk"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)"
[ -n "$version" ] || fail "cannot read the crate version from Cargo.toml"

# rustc applies the last matching prefix: the generic $HOME first.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$rustup_home=/rustup \
--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$crate_dir=/langfuse-cli"

mkdir -p "$dist_dir"
host_arch="$(uname -m)"
for target in "${targets[@]}"; do
    echo "==> $target"
    rustup target add "$target" >/dev/null
    cargo zigbuild --release --locked --target "$target"
    binary="target/$target/release/langfuse"
    [ -f "$binary" ] || binary="$binary.exe"
    [ -f "$binary" ] || fail "no binary was built for $target"
    name="langfuse-$target"
    [[ "$binary" == *.exe ]] && name="$name.exe"
    install -m 0755 "$binary" "$dist_dir/$name"
    if command -v file >/dev/null; then
        file -b "$dist_dir/$name"
    fi
    # Smoke-test the binary that runs on this machine.
    if [[ "$target" == "$host_arch"-* && "$target" == *linux* ]]; then
        "$dist_dir/$name" --version
    fi
done

cd "$dist_dir"
shopt -s nullglob
binaries=(langfuse-*)
[ ${#binaries[@]} -gt 0 ] || fail "no binaries in $dist_dir"
sha256sum "${binaries[@]}" >SHA256SUMS
{
    echo "langfuse-rust-cli $version"
    echo "langfuse-rust-sdk $sdk_version"
    echo "built $(date -u +%Y-%m-%dT%H:%M:%SZ) with $(rustc --version)"
} >VERSION

echo
echo "dist/:"
ls -l "$dist_dir"
cat SHA256SUMS
