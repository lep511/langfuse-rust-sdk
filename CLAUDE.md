# CLAUDE.md

Rust SDK for the Langfuse LLM observability API: async on `reqwest` + `tokio`, Rust 1.99+, edition 2024. It implements every operation of `api/openapi.yml`; the `/api/public/unstable/*` ones are behind the `unstable` feature.

- Workspace members: `macros/` (`langfuse-rust-sdk-macros`, the `#[observe]` proc-macro, published first and pinned with `=x.y.z`) and the unpublished `integration-tests/`.
- `langfuse-cli/` is not a member: it is a standalone crate with its own `CLAUDE.md` and `Cargo.lock` that depends on the SDK release from crates.io. The commands below do not build it.
- Features: `unstable`, `blocking`, `otel`, and on top of `otel`: `macros`, `tracing-layer`, `openai`, `bedrock`. `run_experiment` needs `otel`; batch evaluation and the AI Gateway need no feature.

## Commands

```bash
cargo test                                   # no optional features: iterate with this, or with one feature
cargo test --all-features                    # every feature and the trybuild UI tests: before committing
cargo test --features otel --test otel_tests # one file; a gated file's first line names its feature
cargo test -p langfuse-rust-sdk-integration-tests --all-features  # rig + AWS SDK tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
TRYBUILD=overwrite cargo test --features macros --test observe_ui_tests  # regenerate tests/ui/observe/*.stderr
python3 scripts/generate_blocking.py         # regenerate src/blocking/api.rs
python3 scripts/update_openapi.py --dry-run  # is api/openapi.yml behind Langfuse Cloud? See API-UPDATE-CHECK.md
```

- Use this clippy invocation wherever a skill or tool suggests plain `cargo clippy`, which skips the tests, the examples and every feature-gated module.
- Each feature set builds its own copy of every test and example binary. `cargo clean-own` drops the workspace crates' builds and keeps the dependencies (`docs/build-footprint.md`).
- The `dev` profile has no debuginfo: `CARGO_PROFILE_DEV_DEBUG=line-tables-only` gives backtraces with line numbers.

## Testing

- Tests never touch the network. HTTP behaviour is tested with `wiremock` in `tests/api_<resource>_tests.rs`, with the helpers of `tests/common/mod.rs`.
- A feature-gated test file starts with `#![cfg(feature = "...")]`: without the feature it builds and runs zero tests. In `tests/gateway_tests.rs` only some modules are gated (`otel`, `openai`).
- Config is tested in-crate through `from_lookup`, never by mutating the process environment.
- Tracing tests build tracers with a one-hour `flush_interval`, flush explicitly and parse the OTLP/JSON posted to `/api/public/otel/v1/traces`. Only one test in `tests/observe_tests.rs` touches the global tracer.
- Timing-based tests use short TTLs and delayed mocks, and poll with bounded waits.
- Whatever needs rig, `aws-config` or the AWS SDK's HTTP client goes in `integration-tests/` (its features mirror the main crate's names), never in the main crate's `[dev-dependencies]`: those are compiled for every `cargo test` and trybuild copies all of them into its project. Its test files share `integration-tests/tests/support/`: `tests/common` re-exported, the spans a tracer exported to a fake Langfuse (`tracer`, `exported`) and, under `bedrock`, the fake Bedrock (wiremock responses, event-stream frames, and `slow_bedrock` for a stream that arrives over time).
- Examples hit the real API with `ClientConfig::from_env()` (`.env`). Each doc comment says what the example writes and how it cleans up; feature examples declare `required-features`.
- Every example has a section and an overview row in `examples/EXAMPLES.md` and a row in the README table. `EXAMPLES.md` is also the body of the `cfg(doc)` module `examples`, so its links must be absolute and its code blocks tagged `sh` or `text` (an untagged block becomes a doctest).

## Design constraints

What the code does not show at a glance; the module docs cover the rest.

- **Requests** (`src/http.rs`): the base URL always ends with `/` and paths are joined relative to it, so a base path prefix is kept. Headers resolve default < per-call `CallOptions` < view (`with_options`); timeout and retries resolve crate-private `CallOptions` > view > `CallOptions::default_timeout` > client.
- **Retries** (`src/retry.rs`): non-idempotent requests (POST and PATCH, unless `CallOptions::idempotent` says otherwise, as for OTLP ingestion and the score queue) are retried only on 408, 429, 503 and connection failures. 409 is never retried.
- **Prompt cache**: a method that changes a prompt must invalidate its name, as `create`, `delete*` and `prompt_versions.update` do.
- **Tracing** (`src/otel/`):
  - `Observation` buffers attributes and writes them once at `end`, because the OpenTelemetry SDK's `set_attribute` appends duplicates and caps attributes per span.
  - `flush`/`shutdown` export the spans first, then flush the client, because the export queues media uploads. The exporter's order is: de-duplicate attributes → `upload_media` → `mask_otel_spans` → export.
  - Experiment attributes and `skills_available` never go into baggage.
  - `#[observe]` is the proc-macro in `macros/src/lib.rs` plus its hidden runtime in `src/otel/observe.rs` (`otel::__observe`), which is why both crates share one version.
- **Experiments and batch evaluation**: every task, evaluator and mapper call goes through `catch_panic` (`src/experiment.rs`), so a panic becomes an item or evaluator failure.
- **Python parity**: these ports of the Python SDK must keep its behaviour: `src/skills.rs` (`_client/skills.py`), `src/usage.rs` (`_parse_usage_model`, tested with the cases of `test_parse_usage_model.py`), `src/openai.rs` (`langfuse/openai.py`), and experiment item IDs plus `python_isoformat`/`python_float_repr` (Python's `json.dumps`, `isoformat` and float `repr`).
- **AI Gateway** (`src/gateway.rs`): not part of `api/openapi.yml` or of the blocking client, and gateway keys cannot call the public API. It has its own `reqwest::Client` and retry loop, in which only GET is idempotent, and its own SSE parser over `Response::chunk` (no reqwest `stream` feature).
- **`YamlTool`** (`src/tools/`): on `serde_norway`, not `serde_yml` (RUSTSEC-2025-0068).

## Conventions

- API paths start with `/api/public/`; every interpolated path parameter goes through `encode_path_segment` (`folder/name` → `folder%2Fname`).
- Use the HTTP helper that matches the spec response: a JSON helper for a JSON body, a no-content helper for a 204 (the JSON helpers deserialize every 2xx body).
- Every list method returning `Paginated` / `CursorPaginated` gets a `<method>_pages` companion (params by value, `paginate_pages` / `paginate_cursor`), its params get `impl_page_query!` / `impl_cursor_query!`, and the route is added to the wiring tests in `tests/pagination_tests.rs`.
- A new resource needs: model structs in `src/models/` and a client in `src/api/` (query-param structs next to the client), both registered in their `mod.rs`; a field and its construction in `LangfuseClient` (`src/lib.rs`); the field in `client_with_custom_config` (`tests/config_tests.rs`); a `tests/api_<resource>_tests.rs` wiremock file; serde tests for its enums (`tests/serde_new_resources_tests.rs`, or `tests/serde_unstable_tests.rs`). Unstable sub-clients and their test files are behind `unstable`.
- After adding or changing a public method of an API client, run `python3 scripts/generate_blocking.py`: `src/blocking/api.rs` is generated, never edited by hand, and `tests/blocking_tests.rs` fails when a method has no wrapper.
- Response models: `Deserialize` + `#[serde(rename_all = "camelCase")]`, `Option` for nullable or non-required spec fields. Request models: `Serialize` with `skip_serializing_if = "Option::is_none"`, plus a `new(required...)` constructor (for `..Struct::new(..)`) when they have many optional fields.
- Enums read from responses use `wire_enum!` (`#[non_exhaustive]` plus an `Unknown` variant); request-only enums are plain `Serialize` enums; tagged unions (`#[serde(tag = "...")]`) stay strict.
- Wire names `camelCase` cannot produce get explicit renames (`baseURL`, `row_limit`, filter operators with spaces). JSON-in-query parameters (`filter`, metrics `query`) are `String`s, or typed and serialized with `serde_json::to_string`.
- Public option structs (`GetPromptOptions`, `ScoreQueueOptions`, `TracingOptions`, `PropagatedAttributes`, `GetDatasetOptions`, `ExperimentOptions`, `BatchEvaluationOptions`, `RequestOptions`) are `#[non_exhaustive]` with public fields and one `with_<field>` builder per field: add the builder with every new field. Outside the crate (tests, examples, README, doc tests) build them, and `Evaluation`, `EvaluatorInput` and `LocalExperimentItem`, with their constructor or `Default::default()`, then `with_*(..)`.
- Spec updates follow `API-UPDATE-CHECK.md`. Only `scripts/update_openapi.py` writes `api/openapi.yml` and `api/openapi-updates.txt`; commit a new spec only with the code that implements it, and keep `api/operation-coverage-audit.md` (one row per operation) in step.
- Both published crates inherit `version` from `[workspace.package]` in the root `Cargo.toml`; bump it together with the `=x.y.z` pin on `langfuse-rust-sdk-macros` in `[workspace.dependencies]`.
- Record user-visible changes in `CHANGELOG.md`.
