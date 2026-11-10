# Build footprint: keeping `target/` small

`target/` of this repository used to grow to 57–120 GB. This page records why,
what changed (2026-11-10, branch `build/target-footprint`), the measurements,
and how to keep it small day to day.

Measured on rustc/cargo 1.99.0, aarch64-unknown-linux-gnu (Amazon Linux 2023,
8 vCPU, 30 GB RAM), GNU ld 2.41, every scenario in a fresh `CARGO_TARGET_DIR`.
Sizes are `du -sk` (GiB = 1024³ bytes); times are wall clock.

## Results

| Scenario (fresh target dir) | Before | After |
|---|---|---|
| `cargo build` | 0.71 GiB, 10.8 s | 0.43 GiB, 9.1 s |
| `cargo build --features bedrock` | 1.01 GiB, 14.4 s | 0.58 GiB, 13.4 s |
| `cargo build --all-features` | 1.49 GiB, 27.9 s | 0.83 GiB, 24.5 s |
| `cargo test --no-run` | 9.40 GiB, 75.5 s | **2.00 GiB, 23.2 s** |
| `touch src/lib.rs` + `cargo test --no-run` | 30.9 s | **8.4 s** |
| `cargo test --no-run --all-features` | 22.7 GiB, 194 s | **3.02 GiB, 46.5 s** |
| … then `cargo test --all-features` (adds trybuild) | 23.7 GiB | 3.86 GiB |
| `cargo test --features macros --test observe_ui_tests` | 2.93 GiB, 67.4 s | 1.09 GiB, 28.0 s |
| of which `target/tests/trybuild` | 0.97 GiB | 0.53 GiB |
| `cargo test -p langfuse-rust-sdk-integration-tests --all-features` | (part of the main crate) | 1.17 GiB, 26.9 s |
| one test binary (`api_retry_tests`) | 236 MB | 10 MB |
| packages compiled by `cargo test` (no features) | 267 | 154 |

A cumulative "day of work" in **one** target dir (`cargo test`, `cargo test
--all-features`, `touch src/lib.rs` + `cargo test`, `touch` + `cargo test
--all-features`, `cargo clippy --all-targets --all-features`, `cargo doc
--all-features --no-deps`; after the change also `--workspace` for clippy and
the integration tests) is how the old `target/` reached 57–120 GB:

| Same sequence, one target dir | Before | After |
|---|---|---|
| final size | 34.4 GiB | 8.2 GiB |
| total time | 605 s | 220 s |
| `debug/deps` | 18.1 GiB | 3.1 GiB |
| `debug/examples` | 8.5 GiB | 0.85 GiB |
| `debug/incremental` | 6.5 GiB | 3.3 GiB |
| `tests/trybuild` | 1.08 GiB | 0.84 GiB |

Tests are unchanged: 531 (+2 ignored) for `cargo test`; with all features the
main crate runs 700 (+3 ignored) and `integration-tests` 8, the same 708 tests
by name as before. All 29 examples were run against a Langfuse Cloud test
project before and after, and the traces read back through
`GET /api/public/v2/observations` have the same structure (observation tree,
types, model, usage and cost keys, input/output shape, media references,
scores).

## Root causes, by size

1. **Full DWARF debuginfo in every binary.** The default `dev` profile embeds
   the debuginfo of all dependencies into each of the 50 test binaries and
   31 examples: `api_retry_tests` was 236 MB, of which 7 MB `.text` and
   ~225 MB `.debug_*`. This accounted for 19 GiB of the 22.7 GiB of an
   all-features test build. Fixed by `[profile.dev] debug = false`.
2. **One full copy of every binary per feature set.** A test or example
   binary depends on the library hash, so `cargo test`, `cargo test
   --all-features`, `--features otel`, ... each produce their own set of
   ~80 binaries. Two sets (default + all features) were the bulk of the old
   57 GB `target/`. This is inherent to Cargo; the fix is to make each set
   small (point 1) and to use few feature sets day to day (see below).
3. **Incremental compilation data**, which grew by ~0.6 GiB after one
   edit-rebuild before (2.0 → 2.6 GiB) and was 9.6 GB of the old `target/`.
   It is kept: `incremental = false` saved 0.7–1.2 GiB but made every
   edit-rebuild 28% slower. Remove `target/debug/incremental` when it grows.
4. **Heavy dev-dependencies compiled by every `cargo test`.** rig-core,
   rig-agent, aws-sdk-bedrockruntime with its default features (two HTTP/TLS
   stacks: hyper 0.14 + rustls 0.21, and hyper 1 + rustls 0.23 + aws-lc),
   aws-config with the sso/ssooidc/sts SDKs, aws-smithy-eventstream and a
   full tracing-subscriber were compiled for a plain `cargo test`, although
   only two feature-gated test files and two examples used them. Dev-dep
   feature unification also enabled the AWS SDK's `default` features for the
   optional `bedrock` dependency in every test build. They moved to the
   unpublished member `integration-tests/`: −0.5 GiB, 254 → 145 rlibs and
   37 → 23 s for `cargo test --no-run`.
5. **trybuild builds a second dependency tree.** trybuild copies all
   `[dependencies]` and `[dev-dependencies]` into the project it checks in
   `target/tests/trybuild` (and ignores `[profile]`), so the heavy
   dev-dependencies were checked twice: 0.97 → 0.53 GiB.

## Day-to-day workflow

- Use `cargo test` (or `cargo test --features <the one you work on>`) while
  developing. Every additional feature set is another full copy of all test
  and example binaries; keep `--all-features` for before a commit or release.
- `cargo check` / `cargo clippy` when you only need to know that it compiles
  (no linking, much smaller).
- `cargo test --lib --tests` skips examples and doc tests for quick loops.
- After changing `src/bedrock.rs` or `src/otel/tracing_layer.rs`, and before
  a release: `cargo test -p langfuse-rust-sdk-integration-tests --all-features`.
- Never add rig, aws-config or the AWS SDK's HTTP-client features to the main
  crate's `[dev-dependencies]` (see `integration-tests/Cargo.toml`).

### Debugging

Development builds carry no debuginfo. Panic messages keep their file:line and
backtrace frames keep their function names, but frames have no file:line and a
debugger cannot map to source. For a debugging session:

```sh
CARGO_PROFILE_DEV_DEBUG=line-tables-only RUST_BACKTRACE=1 cargo test --test otel_tests
CARGO_PROFILE_DEV_DEBUG=full cargo test --test otel_tests   # for gdb/lldb
```

This rebuilds with debuginfo; run `cargo clean -p langfuse-rust-sdk`
afterwards to drop those binaries.

### Cleanup

| Command | Removes |
|---|---|
| `cargo clean-own` (alias in `.cargo/config.toml`) | every build of the three workspace crates (all test/example binaries of all feature sets), keeps the compiled dependencies |
| `rm -rf target/debug/incremental` | incremental data (rebuilt on the next build) |
| `rm -rf target/tests/trybuild` | the trybuild project and its dependency tree |
| `cargo clean --doc` | `target/doc` |
| `cargo clean` | everything |

`cargo clean-own` on that 8.2 GiB "day of work" dir removed 6.8 GiB (52,354
files), leaving the compiled dependencies (1.5 GiB) and trybuild; the next
`cargo test --no-run` took 15 s.

[`cargo-sweep`](https://crates.io/crates/cargo-sweep) (not installed here)
can remove artifacts older than N days (`cargo sweep --time 14`) or not built
by the installed toolchains (`cargo sweep --installed`).

### rust-analyzer

rust-analyzer's check-on-save runs `cargo check --workspace --all-targets` in
`target/`; with all features it would compile rig and the AWS SDK on every
save. Recommended user settings (VS Code `settings.json`; nothing is committed):

```json
{
  "rust-analyzer.cargo.targetDir": true,
  "rust-analyzer.check.command": "check",
  "rust-analyzer.cargo.features": []
}
```

- `cargo.targetDir: true` puts its artifacts in `target/rust-analyzer`, so it
  never blocks on or invalidates command-line builds.
- `check.command: "check"` (the default; `clippy` costs more on every save).
- A fixed, small `cargo.features` (e.g. `["otel"]` when working on tracing),
  not `"all"`. Files gated on other features show as inactive code.

### Faster linking (optional, per machine)

rust-lld links much faster than GNU ld here; all test suites pass the same
(531, 700 and 8 tests):

| | GNU ld | rust-lld |
|---|---|---|
| `touch src/lib.rs` + `cargo test --no-run` | 8.1–8.4 s | 3.1–3.2 s |
| `cargo test --no-run` (fresh) | 22.4–22.9 s | 17.3–17.7 s |
| `cargo test --no-run --all-features` (fresh) | 45.5–47.0 s | 34.6–34.8 s |
| size of those target dirs | 2.00 / 3.02 GiB | 2.03 / 3.18 GiB |

On aarch64 Linux the self-contained form (`-Clinker-features=+lld
-Clink-self-contained=+linker`) still needs `-Z unstable-options`, so the
stable way points gcc at the `ld.lld` that rustup ships. The path is specific
to the machine and toolchain, so it belongs in your user configuration
(`~/.cargo/config.toml`), not in this repository:

```toml
[target.aarch64-unknown-linux-gnu]
rustflags = [
    "-Clink-arg=-fuse-ld=lld",
    # `rustc --print sysroot` + /lib/rustlib/aarch64-unknown-linux-gnu/bin/gcc-ld
    "-Clink-arg=-B/home/<user>/.rustup/toolchains/stable-aarch64-unknown-linux-gnu/lib/rustlib/aarch64-unknown-linux-gnu/bin/gcc-ld",
]
```

Check with `readelf -p .comment target/debug/deps/<test>-<hash> | grep LLD`.
Changing rustflags rebuilds everything once. trybuild overrides rustflags, so
it keeps using the default linker (it only runs `cargo check` anyway).

## `aws-sdk-bedrockruntime`: kept, minimized

- **It is part of the public API.** `bedrock::Bedrock::new` takes an
  `aws_sdk_bedrockruntime::Client`, `converse`/`converse_stream` take the
  SDK's fluent builders, and `send`/`recv` return `ConverseOutput`,
  `SdkError<ConverseError>` and the stream event types. Replacing the SDK is
  a breaking change.
- **Langfuse needs nothing from it beyond the call.** A Bedrock call is a
  generation with the model ID, the messages, the inference parameters, the
  output and the usage (input/output/total, plus cache reads and writes);
  the crate extracts these from the SDK types and exports them over OTLP.
- **Its cost in the library is small.** The `bedrock` dependency is
  `default-features = false`: the application brings its own client, HTTP
  stack and runtime. `cargo build --features bedrock` adds 0.15 GiB and
  3–4 s over `cargo build` (43 more crates; `aws-sdk-bedrockruntime` itself
  compiles in ~3 s). The large costs were the dev-dependency with default
  features (two TLS stacks, aws-lc C build, legacy hyper) and aws-config,
  both now only in `integration-tests/`.
- **Alternatives, not applied:**
  - *A thin client on reqwest* (`aws-sigv4` for SigV4, JSON for Converse, an
    event-stream parser for ConverseStream). It would save part of those
    0.15 GiB in `--features bedrock` builds (aws-sigv4 itself depends on
    `aws-smithy-*` crates). In exchange it breaks the API and moves the
    credential chain (environment, profiles, SSO, IMDS, the Bedrock bearer
    token), region and endpoint resolution, retries and stream error handling
    into this crate.
  - *A separate published `langfuse-rust-sdk-bedrock` crate.* It breaks
    users of the `bedrock` feature. Since the main crate's tests no longer
    compile the SDK unless `bedrock` is enabled, it would save only those
    0.15 GiB in `--all-features` builds.
  - Both would warrant a 0.11.0 release.

## Langfuse documentation consulted

- [OpenTelemetry (OTEL) for LLM Observability](https://langfuse.com/integrations/native/opentelemetry):
  the OTLP endpoint `/api/public/otel/v1/traces`, OTLP over HTTP with JSON or
  protobuf (no gRPC), Basic auth, `x-langfuse-ingestion-version: 4`, and the
  `langfuse.*` / `gen_ai.*` attribute mapping.
- [Amazon Bedrock Integration](https://langfuse.com/integrations/model-providers/amazon-bedrock):
  what a Bedrock Converse generation carries (model ID, messages, inference
  parameters, output, usage input/output/total).
- [API reference, `POST /api/public/otel/v1/traces`](https://api.reference.langfuse.com):
  `application/x-protobuf` or `application/json`, gzip with
  `Content-Encoding: gzip`.

No change touches what is sent to Langfuse. The export path (OTLP/JSON, gzip
through `flate2`, Basic auth through `base64`, media hashing through `sha2`)
is unchanged.
