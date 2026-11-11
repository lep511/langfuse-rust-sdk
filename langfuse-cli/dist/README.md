# Prebuilt `langfuse` binaries

Static Linux binaries of `langfuse-rust-cli`. They need no Rust, no Node.js
and no system libraries: they are linked statically against musl and carry
their own TLS root certificates. `VERSION` names the CLI and SDK versions they
were built from.

| File | Platform |
|---|---|
| `langfuse-x86_64-unknown-linux-musl` | Linux x86_64 (Intel/AMD) |
| `langfuse-aarch64-unknown-linux-musl` | Linux ARM64 (Graviton, Ampere, Raspberry Pi 64-bit) |

## Download

Pick the file that matches `uname -m` (`x86_64` or `aarch64`):

```sh
arch="$(uname -m)"   # x86_64 or aarch64
base=https://github.com/lep511/langfuse-rust-sdk/raw/main/langfuse-cli/dist
curl -fsSLo langfuse "$base/langfuse-$arch-unknown-linux-musl"
curl -fsSLO "$base/SHA256SUMS"
echo "$(grep "langfuse-$arch-unknown-linux-musl" SHA256SUMS | cut -d' ' -f1)  langfuse" | sha256sum -c -
chmod +x langfuse
sudo mv langfuse /usr/local/bin/
langfuse --version
```

Then set the credentials as with the SDKs (`LANGFUSE_PUBLIC_KEY`,
`LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL`), or pass `--env .env`. See the
[CLI README](../README.md).

## Rebuilding

From `langfuse-cli/`, with rustup, [zig](https://ziglang.org/download/) and
`cargo install cargo-zigbuild --locked`:

```sh
scripts/build-dist.sh                              # both targets
scripts/build-dist.sh x86_64-unknown-linux-musl    # one target
```

The script cross-compiles with `cargo zigbuild --release --locked`, copies
the binaries here, and rewrites `SHA256SUMS` and `VERSION`. Every committed
rebuild adds about 11 MB to the repository history, so commit new binaries
only for a new CLI version.
