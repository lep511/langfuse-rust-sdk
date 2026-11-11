# langfuse-rust-cli

A command-line client for the [Langfuse](https://langfuse.com) API, written in
Rust on top of [`langfuse-rust-sdk`](https://crates.io/crates/langfuse-rust-sdk).
It is a port of the official TypeScript CLI ([`@langfuse/cli`](https://github.com/langfuse/langfuse-cli)):
the same `langfuse api <resource> <action>` commands, the same `skills`
workflow, the same exit codes. It ships as one native binary (about 5 MB, no
Node.js) and starts in milliseconds.

- Every operation of the Langfuse public API (120, of which 21 are
  `/api/public/unstable/*`) is a typed [clap 4](https://docs.rs/clap) command.
  The command names and aliases match the official CLI's.
- Requests go through the SDK's HTTP client, which handles authentication,
  timeouts, retries with backoff, and errors. The response is printed exactly
  as the server sent it.
- `langfuse skills install|update|push` installs Langfuse-hosted skills into a
  project, tracked in `langfuse-skills.lock`, and publishes local ones.
  `langfuse get-skill` prints the latest Langfuse agent skill.

## Install

### Prebuilt binary (Linux, no Rust needed)

```sh
curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sh
```

[`install.sh`](install.sh) picks the binary for the machine (x86_64 or ARM64)
from [`dist/`](dist/), checks it against `SHA256SUMS`, and installs it in
`/usr/local/bin` when that directory is writable, else in `~/.local/bin`.

The script is POSIX sh, so `| bash` works the same (as do `dash` and busybox
`sh`, e.g. on Alpine); `| sh` also works where bash is not installed. Options
go after `-s --` with either shell:

```sh
curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | bash
curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | bash -s -- --dir ~/bin
curl -fsSL https://raw.githubusercontent.com/lep511/langfuse-rust-sdk/main/langfuse-cli/install.sh | sudo bash   # system-wide
```

`LANGFUSE_INSTALL_DIR` chooses the directory too, like `--dir`.

Or download a binary by hand:

```sh
arch="$(uname -m)"   # x86_64 or aarch64
curl -fsSLo langfuse "https://github.com/lep511/langfuse-rust-sdk/raw/main/langfuse-cli/dist/langfuse-$arch-unknown-linux-musl"
chmod +x langfuse && sudo mv langfuse /usr/local/bin/
```

[`dist/README.md`](dist/README.md) also shows how to verify the checksum and
how to rebuild the binaries with `scripts/build-dist.sh`.

### From source

```sh
cargo install --path langfuse-cli          # builds the `langfuse` binary
cargo install --path langfuse-cli --no-default-features   # without the unstable API
```

The crate is not published on crates.io yet. It needs Rust 1.99 or later and
downloads `langfuse-rust-sdk` from crates.io.

## Authentication

The CLI uses a project API key pair, like the SDKs:

```sh
export LANGFUSE_PUBLIC_KEY=pk-lf-...
export LANGFUSE_SECRET_KEY=sk-lf-...
export LANGFUSE_BASE_URL=https://cloud.langfuse.com   # default; LANGFUSE_HOST also works
```

You can also pass them as flags or from a dotenv file. Precedence: flag, then
`--env` file, then the environment.

```sh
langfuse --env .env api prompts list
langfuse --public-key pk-lf-... --secret-key sk-lf-... --host https://us.cloud.langfuse.com api prompts list
```

## Usage

```sh
# Discovery
langfuse api help
langfuse api prompts help              # or: langfuse api help prompts
langfuse api prompts create help       # or: --help, or: langfuse api help prompts create
langfuse --json api schema             # machine-readable schema of every command

# Prompts
langfuse api prompts list
langfuse api prompts get my-prompt --label staging
langfuse api prompts create --type text --name my-prompt --prompt 'Hello {{name}}'
langfuse api prompts create --type chat --name support \
  --prompt '{"role":"system","content":"be nice"}' \
  --prompt '{"role":"user","content":"{{question}}"}'
langfuse api prompts update-version my-prompt 3 --new-labels production

# Any body, unchanged: --body-json or --body-file (- reads stdin)
langfuse api datasets create --body-json '{"name":"qa","description":"Q&A pairs"}'
langfuse api scores create --body-file scores.json

# Observations, scores, datasets
langfuse api observations list --trace-id <trace-id> --limit 10
langfuse api scores list --data-type NUMERIC --from-timestamp 2026-11-01T00:00:00Z
langfuse api dataset-items list --dataset-name qa

# Every page of a list (page or cursor pagination), at most 1000 items by default
langfuse api observations list --all
langfuse api observations list --limit 100 --all --max-items 5000

# Preview the request without sending it
langfuse api observations list --limit 5 --curl
```

OpenAPI tags and route versions also work as resource names (`scores-v3 list`,
`prompts-v2 get`, `trace delete`, …), as in the official CLI.

### Arguments

- Path parameters are positional (`prompts get <PROMPT_NAME>`) and are
  percent-encoded (`folder/name` becomes `folder%2Fname`).
- Query and body fields are kebab-case flags (`--from-start-time` for
  `fromStartTime`). Some flags are renamed so they do not collide:
  `--provider-secret-key` sends the `secretKey` of an LLM connection, and
  `--predefined-public-key` / `--predefined-secret-key` send those of an API
  key.
- Values are checked before anything is sent:
  - integers and numbers;
  - RFC 3339 date-times (`2026-11-01T00:00:00Z`);
  - JSON for `--filter`, `--config` and similar flags;
  - enums, with a suggestion when you mistype one.
- Booleans: `--resolve` or `--resolve=true`, and `--resolve=false` or
  `--no-resolve`.
- Arrays are repeatable flags (`--labels a --labels b`). Body arrays also accept
  one JSON array (`--labels '["a","b"]'`).
- Union bodies (`prompts create`, `evaluators create`, `unstable-dashboards
  add-placement`): `--type` selects the variant, and with it the variant's
  required fields.
- `--body-json` and `--body-file` work on every operation with a body. They
  cannot be combined with field flags.

### Output

| Option | Output |
|---|---|
| (default) | The response body as pretty JSON, keys in the server's order, coloured on a terminal |
| `--json` | One line `{"status", "headers", "body"}` |
| `--output <file>` | The body written to a file, nothing on stdout |
| `--format table` | Lists as a table sized to the terminal width (`--columns name,labels,usage.total`), an object as field/value rows |
| `--format ndjson` | One compact line per list item; with `--all` the items are written as pages arrive |
| `--curl` | The request as a `curl` command (credentials redacted unless `--show-secrets`) |

Colours follow `--color auto|always|never` and `NO_COLOR`. When the output is
piped or redirected it is plain JSON, byte for byte the official CLI's format.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Successful API response or local command |
| 1 | Unexpected internal failure |
| 2 | Invalid command or input (usage); no request sent |
| 3 | Missing or invalid configuration or credentials; no request sent |
| 4 | Network, DNS, TLS or timeout failure reaching the host |
| 5 | The API responded with a non-success HTTP status (the response is still printed) |
| 6 | Local file failure |
| 7 | Invalid or inconsistent API response |

## Skills

```sh
langfuse skills install skill-name            # the production label
langfuse skills install skill-name@staging    # a label
langfuse skills install skill-name@3          # an immutable version
langfuse skills install --tag support
langfuse skills install --all
langfuse skills install                       # restore the versions recorded in the lockfile
langfuse skills update                        # follow the saved selectors

langfuse skills push ./skills/skill-name --dry-run --json
langfuse skills push ./skills/skill-name --label production --commit-message "…"
langfuse skills push ./skills --recursive --update-only
langfuse skills push ./skills/skill-name --exclude credentials.json --exclude private/
```

- Skills are installed in `.agents/skills/<name>`; change the parent with
  `--directory`. They are recorded in `langfuse-skills.lock`; `--no-lockfile`
  skips the lockfile.
- An installation is staged, verified file by file (size and SHA-256), then
  swapped in atomically. A directory with local changes is only replaced with
  `--force`. `--force` does not bypass manifest validation (exit 7).
- `push` publishes the files Git lists in a checkout: tracked files plus
  untracked files that are not ignored. Outside Git it publishes every
  supported text file. `--exclude` leaves out literal paths, and `.git` /
  `.gitignore` are never sent.

## Differences from the official CLI

- **API versions.** Only the API spec of the SDK is supported: Langfuse 4.71.0
  plus the newer Topics operations. There are no bundled snapshots and no
  `--api-version`.
  - `api versions` prints the spec this CLI targets.
  - `api versions detect` prints the server version from `/api/public/health`.
- **Option placement.** `--curl`, `--show-secrets`, `--output`, `--format` and
  `--columns` go after `api`, for example `langfuse api … --curl`. Global
  options work anywhere.
- **Retries.** Failed requests are retried (`--max-retries`, default 2) with
  exponential backoff that honours `Retry-After`. POST requests are retried
  only on 408, 429, 503 and connection failures. `--max-retries 0` behaves like
  the official CLI.
- **Stricter input.**
  - Enum values and date-times are validated before sending.
  - `--json=false` is an error; the official CLI turns JSON output on for it.
  - `--json-body` is accepted as an alias of `--body-json`, as the official
    README spells it.
- **Field flags.** Field flags are added on `traces delete-many`
  (`--trace-ids`) and `prompts update-version` (`--new-labels`).
- **Ingestion header.** `otel export-traces` sends
  `x-langfuse-ingestion-version: 4`, which the spec requires for real-time
  ingestion.
- **Schema output.** `api schema --json` has the official shape plus `cli` and
  `sdk` versions. Parameters are described by their CLI kind (`integer`,
  `string`, `json`, …).
- **Skills.**
  - Lockfile entries are sorted by path bytes, not by locale collation.
  - YAML aliases in a SKILL.md frontmatter are resolved, with libyaml's
    limits, instead of rejected.
  - File contents are downloaded four batches at a time.

## Development

The crate is independent of the SDK workspace that contains it: it has its own
`[workspace]` and `Cargo.lock`, and depends on the SDK release from crates.io.

```sh
cd langfuse-cli
cargo test                                   # with the unstable API (default)
cargo test --no-default-features             # without it
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release                        # target/release/langfuse
```

See [CLAUDE.md](CLAUDE.md) for the code layout and how to add an operation.

## License

MIT, see [LICENSE](LICENSE).
