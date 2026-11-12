# Changelog

## 0.2.0 (2026-11-12)

The result of an audit of how the CLI uses Rust
(`../docs/langfuse-cli-rust-audit.md`). Machine-readable output is unchanged,
byte for byte.

- **Pipes.** A reader that closes the pipe (`langfuse api … | head`) ends a
  printing command quietly with exit 0, and `--all` stops sending requests;
  `skills` commands finish their work. Before, `completions <shell> | head`
  panicked (exit 101) and `api schema --json | head` failed with exit 1.
- **Exit codes.** Only the documented ones (0–7): a panic is caught and
  becomes exit 1, and a write to stdout that fails for any reason other than
  a closed pipe (a full disk behind a redirection) is exit 1 with a message,
  for `skills` commands too.
- **Memory.** Responses are serialized straight to stdout and `--all` moves
  each page instead of copying it: about half the peak memory with `--all`,
  a quarter to a third less for one large response.
- **`api schema`** (without `--json`) no longer prints colour codes when its
  output is not a terminal; it follows `--color`.
- **Skills.**
  - Manifests are requested a few at a time; results keep their order.
  - A file whose name is not valid UTF-8 is an explicit error when
    publishing, instead of a "file not found".
  - The details of lockfile errors are worded differently (the
    `Cannot read <path>:` prefix and exit 6 are the same).
- **Completions.** The zsh and fish scripts complete file names for
  `--directory` and `--lockfile`.
- **Library (breaking).** The crate's public API is now `run`, `command`,
  `Io` and `Exit`; its modules are private. `Io` has a new `color_allowed`
  field and an `Io::stdio` constructor.
- **Checks.** A `[lints]` table (no `unsafe`, clippy lints for needless
  clones and lossy casts) and a CI workflow
  (`../.github/workflows/langfuse-cli.yml`).

## 0.1.0 (2026-11-11)

First version: a Rust port of the official Langfuse CLI (`@langfuse/cli`
1.3.1), built on `langfuse-rust-sdk` 0.11.3 and clap 4.6.

- **`langfuse api`.** All 120 operations of the API spec of 2026-11-11.
  - The 21 `/api/public/unstable/*` operations are behind the default
    `unstable` feature.
  - Command names and aliases are the official CLI's: the 4.71.0 golden plus
    the 4 new Topics operations.
  - Every command takes typed arguments, built into the SDK's query and
    request types.
  - Responses are printed unchanged.
  - Also: `--body-json` / `--body-file`, `--all` / `--max-items`, `--curl`,
    `--json`, `--output`, and `api schema --json`.
- **Output formats.** `--format table` (sized to the terminal) and
  `--format ndjson` (streamed with `--all`). JSON is coloured on terminals.
- **Requests.** Retries with backoff (`--max-retries`), and `--timeout`.
- **Exit codes.** The official CLI's (0–7).
- **`langfuse skills install|update|push`** with the official lockfile format,
  manifest validation, atomic installs and Git-aware publishing.
- **`langfuse get-skill`** and **`langfuse completions <shell>`**.
- **Prebuilt binaries** in `dist/`: static Linux x86_64 and ARM64 binaries,
  built by `scripts/build-dist.sh` (cargo-zigbuild), with `SHA256SUMS`.
- **`install.sh`**: `curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh` installs the binary for the
  machine after checking its SHA-256.
- **`api traces` help** says that reading traces is deprecated (the trace read
  endpoints leave Langfuse Cloud on 2026-11-16). It shows the equivalent
  `observations list` commands and links the migration guide; `delete` and
  `delete-many` are unchanged.
