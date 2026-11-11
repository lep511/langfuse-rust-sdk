# Changelog

## 0.1.0 (unreleased)

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
