# CLAUDE.md

`langfuse-rust-cli` (binary `langfuse`) is a Rust port of the official Langfuse
CLI (`@langfuse/cli`, TypeScript, in `../../langfuse-cli`). It is built on
`langfuse-rust-sdk` from crates.io and on clap 4 derive. It is a standalone
crate: it has its own `[workspace]` and `Cargo.lock`, is not a member of the
SDK workspace around it, and must keep depending on the SDK by version, never
by `path`.

## Commands

```bash
cargo test                                   # default features (unstable API included)
cargo test --no-default-features             # without the unstable operations
cargo test --lib                             # in-crate unit tests
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo build --release
scripts/build-dist.sh                        # static Linux binaries (x86_64, ARM64) into dist/, with SHA256SUMS and VERSION
```

The `[lints]` table of `Cargo.toml` forbids `unsafe` and turns on the clippy lints for needless clones and lossy casts; with `-D warnings` they are errors. `../.github/workflows/langfuse-cli.yml` runs the commands above (all but the release build) and `cargo audit` on pushes and pull requests that touch `langfuse-cli/`.

`dist/` is committed so the binaries can be downloaded from GitHub (`dist/README.md`); every committed rebuild adds about 11 MB to the history, so rebuild it only for a new version. `scripts/build-dist.sh` needs zig and `cargo-zigbuild` (`ring` compiles C for musl), and remaps local paths out of the binaries. `install.sh` (POSIX sh, run as `curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh`) downloads the binary for the machine from `dist/` on main, verifies it against `SHA256SUMS` and installs it; `LANGFUSE_CLI_BASE_URL=file://$PWD/dist sh install.sh --dir /tmp/x` tests it locally. `dist/`, `scripts/` and `install.sh` are excluded from the crate package.

## Layout

- `src/lib.rs`: `run(args, io)`, the whole CLI on injectable streams (`Io`, which also carries the working directory and whether the environment allows colours), plus `command()`. They, `Io` and `Exit` are the crate's public API; the modules are private, so the compiler reports unused items.
  - `command()` is the clap tree, with each action's long help naming its HTTP call; before parsing, `run` rewrites `api <r> <a> help` to `--help`.
  - `run` catches panics (exit 1) and flushes stdout.
- `src/cli.rs`: root parser and global options.
- `src/config.rs`: precedence flag > `--env` file > environment > default, resolved by `resolve(globals, lookup)`. Tests never touch the process environment.
- `src/error.rs`: `CliError` and the exit codes (`Exit`, 0–7). `CliError::stdout` is a failed write to stdout: a closed pipe ends the command quietly with exit 0, anything else is exit 1.
- `src/operation.rs`: `OperationMeta` (operationId, method, path template, pagination, auth, response kind, body kind, extra headers, table columns), the `Operation` trait and `PreparedRequest`. Query strings are encoded with `serde_urlencoded` as in the SDK; path segments use the SDK's unreserved set. The `const fn` builder of `OperationMeta` asserts what the executor needs, so an impossible operation does not compile.
- `src/execute.rs`: sends a `PreparedRequest` through the SDK's `http::HttpClient` with `serde_json::Value` bodies.
  - A response hook captures status and headers.
  - GET and DELETE-with-query pass the query as typed parameters, which keeps repeated names; other methods pass it through `RequestOptions`.
  - `render_curl` builds the curl preview from the same request.
- `src/paginate.rs`: `--all`, a port of the official `callAllPages`: 100 requests at most, `--max-items`, the same stderr notices, NDJSON streaming. Each page is moved into the result, not cloned.
- `src/output.rs` and `src/table.rs`: `Io` and its stdout methods (`out`, `out_with`, `status`), the official output formats serialized straight into buffered stdout, coloured JSON, `--format table|ndjson`, and `escape_terminal_text`.
- `src/schema.rs`: `api schema --json` and `registry()`, built from the clap tree plus `OperationMeta`.
- `src/api/`: one file per resource, plus `mod.rs`:
  - the macros `resource!`, `resources!`, `path_op!`, `json_op!`, `page_list_op!` and `cursor_list_op!` (they check at compile time that each path parameter they fill is in the path);
  - the `Resource` enum, where unstable resources are behind `cfg(feature = "unstable")`;
  - the `schema` and `versions` built-ins.
- `src/skills/`: a port of the official `src/skills*.ts`.
  - HTTP goes through the SDK's typed `client.skills` methods, on `api::Remote`, whose methods take `&self`: manifests and file contents are fetched a few at a time, in order.
  - Every manifest is validated like the official zod schemas; a mismatch is exit 7.
  - `types.rs` has `SkillName` and `Version`: building one is the validation, so nothing checks a name or a version twice.
  - `lock.rs` is the strict lockfile (derived serde, objects only), `install.rs` covers install and update, and `push.rs` with `local_skill.rs` and `publish_filter.rs` covers push and `git ls-files` (a `tokio::process` child with a timeout).
  - The `--json` results are `Serialize` enums (`Report` in `install.rs` and `push.rs`); the order of their fields is the order of the keys.
  - `paths::Temporary` removes a staging directory or a temporary file when dropped.

## Adding or changing an operation

1. **Write the action** in `src/api/<resource>.rs`: a `#[derive(Args)]` struct, or one of the macros for path-only, JSON-only and plain list operations.
2. **Implement `Operation`.** `META` names the canonical resource and action, which must be the official CLI's names. `prepare` builds the `PreparedRequest`:
   - Query: fill the SDK's params struct (`ListPromptsParams`, `PageParams`, …).
   - Body: use the SDK's request type when it can be built from the flags. Otherwise use `args::ObjectBody`, because the SDK's request types are serialize-only.
3. **Field flags** go in a struct with `#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]` and `#[command(next_help_heading = "Request body")]`, flattened next to `args::BodyChannel`.
   - Required fields use `required_unless_present_any = ["body_json", "body_file"]`.
   - Booleans are `Option<bool>` with `num_args = 0..=1, require_equals = true, default_missing_value = "true"`, plus a hidden `--no-<flag>` combined with `args::tri_state`.
   - A flag whose values are one of the SDK's wire enums has the enum's type and `value_parser = args::wire::<T>(VALUES)`; `args::required` unwraps a flag that clap requires only without `--body-json` / `--body-file`.
4. **Register it** as a variant of the resource's `resource!` enum. The variant's `///` comment is the help text.
5. **Test it.** Update `tests/fixtures/commands.json` when the operation is new; `tests/commands_parity_tests.rs` compares it with `api schema --json`. Add a wiremock case to `tests/api_tests.rs` for the method, path, query and body. `tests/operations_tests.rs` already runs every operation with `--curl` and checks its method and path.

The variant name becomes the action name in kebab case. Resource aliases are `#[command(alias = …)]` on the `Resource` variant. An alias that covers only some actions is a hidden resource with its own `resource!` enum, like `scores-v3` and `prompt-version`.

## Conventions

- Output that machines read keeps the official CLI's bytes: pretty JSON with 2 spaces, the `--json` envelope, the skills JSON key order, and the stderr notices of `--all` and skills. The `*_output_bytes` tests of `tests/skills_tests.rs` pin the skills output byte for byte.
- Everything written to stdout goes through `Io::out`, `Io::out_with` or, for the status lines of `skills`, `Io::status`: never `write!(io.stdout, …)`. A closed pipe then ends a printing command quietly and lets a `skills` command finish its work.
- Text built from server or file data goes through `escape_terminal_text` before the CLI adds its own line breaks.
- Errors carry an exit code. Local validation (exit 2 and 6) runs before credentials are checked (exit 3), and both before any request.
- When the SDK version changes, update `[dependencies]`, `execute::SDK_VERSION` (a test compares it with `Cargo.lock`) and `api::SPEC`.
- Record user-visible changes in `CHANGELOG.md`.
