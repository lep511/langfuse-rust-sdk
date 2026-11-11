# CLAUDE.md

Rust SDK for the Langfuse LLM observability API: async on `reqwest` + `tokio`, Rust 1.99+, edition 2024. It implements all 120 operations of `api/openapi.yml`; the 21 `/api/public/unstable/*` ones are behind the `unstable` feature. Workspace members: `macros/` (`langfuse-rust-sdk-macros`, the `#[observe]` proc-macro, published first and pinned with `=x.y.z`) and the unpublished `integration-tests/`.

Features: `otel` (native OpenTelemetry tracing in `src/otel/`, and `run_experiment`); on top of it `macros` (`#[observe]`), `tracing-layer` (bridge from the `tracing` crate), `openai` (traced `async-openai` 0.43) and `bedrock` (traced `aws-sdk-bedrockruntime` Converse calls); `blocking` (blocking client); `unstable`. Datasets, evaluators, batch evaluation and the AI Gateway client need no feature.

## Commands

```bash
cargo test                                   # without optional features (551 tests)
cargo test --all-features                    # every feature (727, includes the trybuild UI tests)
cargo test -p langfuse-rust-sdk-integration-tests --all-features  # rig + AWS SDK tests (8)
cargo test --features otel --test otel_tests # one file; a gated file's first line names its feature
cargo test --features openai --test gateway_tests  # gateway: some modules need otel or openai
cargo test --lib                             # in-crate unit tests (--features otel adds the otel ones)
cargo clippy --workspace --all-targets --all-features -- -D warnings
TRYBUILD=overwrite cargo test --features macros --test observe_ui_tests  # regenerate tests/ui/observe/*.stderr
cargo run --example health_check             # examples hit the real API; features are in their [[example]]
python3 scripts/generate_blocking.py         # regenerate src/blocking/api.rs
python3 scripts/update_openapi.py            # download api/openapi.yml from Langfuse Cloud, report what is new and append the dated report to api/openapi-updates.txt (--dry-run: print only)
```

Use this clippy invocation wherever a skill or tool suggests plain `cargo clippy`: without `--all-targets --all-features` it skips the tests, the examples and every feature-gated module. Each feature set builds its own copy of every test and example binary: iterate with `cargo test` or a single feature, run `--all-features` before committing. `cargo clean-own` (alias in `.cargo/config.toml`) drops the workspace crates' builds and keeps the dependencies. The `dev` profile has no debuginfo; `CARGO_PROFILE_DEV_DEBUG=line-tables-only` gives backtraces with lines, `full` a debugger (`docs/build-footprint.md`).

## Testing

- Tests never touch the network. HTTP behaviour is tested with `wiremock` in `tests/api_<resource>_tests.rs`; shared helpers are in `tests/common/mod.rs` (`test_client()`, `test_client_with_retries(n)`, `json(status, body)`, `single_request`, `body_json`/`body_bytes` (they gunzip bodies sent with `Content-Encoding: gzip`, as OTLP exports are by default), `query_pairs`, `query_value`).
- Feature-gated test files start with `#![cfg(feature = "...")]`; without that feature they build and run zero tests.
- Config is tested in-crate through `from_lookup`, never by mutating the process environment.
- Tracing tests build tracers with a one-hour `flush_interval`, flush explicitly and parse the OTLP/JSON posted to `/api/public/otel/v1/traces`. Only one test in `tests/observe_tests.rs` touches the global tracer.
- Timing-based tests (prompt cache, concurrent media uploads) use short TTLs and delayed mocks and poll with bounded waits.
- The `openai`, `bedrock` and rig tests run the real client crates against a second wiremock server (SSE bodies for streams; Bedrock with static credentials, retries disabled and frames encoded by `aws-smithy-eventstream`).
- `tests/blocking_tests.rs` uses plain `#[test]`s and fails when a public method of `src/api` has no blocking wrapper.
- `integration-tests/` holds whatever needs rig or the AWS SDK with an HTTP client; run it with `-p langfuse-rust-sdk-integration-tests --features <feature>`. Its features mirror the main crate's names (`otel`, `tracing-layer`, `bedrock`), its heavy crates are optional normal dependencies (dev-dependencies cannot be optional), and its tests include `tests/common` with `#[path = "../../tests/common/mod.rs"]`. Never add rig, aws-config or the AWS SDK's HTTP-client features to the main crate's `[dev-dependencies]`: dev-dependencies are compiled for every `cargo test` whatever the features, and trybuild copies all of them into its project.
- Examples in `examples/` hit the real API and build their client with `ClientConfig::from_env()` (`LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL` from `.env` or the environment; the gateway examples also `LANGFUSE_GATEWAY_KEY`/`LANGFUSE_GATEWAY_HOST`). Each doc comment says whether the example writes data and how it cleans up; feature examples declare `required-features` in `Cargo.toml`.

## Architecture

**Client.** `LangfuseClient` (`src/lib.rs`, `#[non_exhaustive]`, `Clone`) holds one sub-client per resource group (`src/api/`, one file each, query-param structs next to the client), all sharing an `Arc<HttpClient>`; the project ID, prompt cache and background tasks are `Arc`-shared by clones. `ClientConfig` (`src/config.rs`) comes from the builder or `from_env` (`LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL`/`LANGFUSE_HOST`, `LANGFUSE_TIMEOUT`, `LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT`). The base URL always ends with `/` and request paths are joined relative to it, so a base path prefix is kept.

**HTTP** (`src/http.rs`). One private `execute` loop handles auth, headers, query (`serde_urlencoded`), JSON body, the `response_hook` and retries. Headers resolve default < per-call `CallOptions` < view, by name; a view (`LangfuseClient::with_options`, `RequestOptions`) also replaces same-named query parameters and merges its `body` fields into JSON object bodies. Timeout and retries resolve crate-private `CallOptions` (set by `get_cached`, batch evaluation, ingestion, the score queue) > view > `CallOptions::default_timeout` (60 s for `observations.list_pages`, 5 s for `get_cached` unless a client timeout is configured) > client. Helpers come in two families: JSON responses (`get`, `get_with_query`, `post`, `put`, `patch`, `delete_with_response`, `delete_with_body_response`) and ignored bodies (`post_no_response`, `post_no_response_with_headers`, `patch_no_content`, `delete_no_content`, `delete_no_content_with_query`, `delete_with_body`).

**Retries** (`src/retry.rs`): exponential backoff with jitter, honouring `Retry-After`. 408, 429, 503 and connection failures are retried for every request; other 5xx, timeouts and dropped connections only for idempotent ones (GET/HEAD/PUT/DELETE/OPTIONS, unless `CallOptions::idempotent` overrides it, as OTLP ingestion and the score queue do). 409 is never retried.

**Errors** (`src/error.rs`): `LangfuseError` (`#[non_exhaustive]`) maps each status to a variant (`Conflict`, `PreconditionFailed`, `RateLimited`, `ServiceUnavailable`, other 4xx `ClientError`, other 5xx `ServerError`, …); `message()` takes the body's `message` or `error`, else the body or the status reason.

**Models** (`src/models/`):
- Responses: `Deserialize` + `#[serde(rename_all = "camelCase")]`, `Option` for nullable or non-required spec fields. Requests: `Serialize` with `skip_serializing_if = "Option::is_none"`; structs with many optional fields get a `new(required...)` constructor for `..Struct::new(..)`.
- Enums read from responses use `wire_enum!` (`src/models/macros.rs`: `#[non_exhaustive]` plus an `Unknown` variant); request-only enums are plain `Serialize` enums; tagged unions (`#[serde(tag = "...")]`, shared base fields `#[serde(flatten)]`) stay strict. Constant fields use `#[serde(tag = "...", rename = "...")]` on the struct.
- Wire names `camelCase` cannot produce get explicit renames (`baseURL`, `row_limit`, filter operators with spaces, …). JSON-in-query parameters (`filter`, metrics `query`) are `String`s, or typed and serialized with `serde_json::to_string`. `ListObservationsParams` serializes as key/value pairs so `environment` can repeat.
- Prompt compilation (`Prompt::variables`, `compile*`, the private `{{variable}}` template engine) lives in `src/models/prompts.rs`.

**Prompt cache** (`src/prompt_cache.rs`, behind `PromptsClient::get_cached`): expired entries stay readable while one background task refreshes the key; a generation counter bumped by `invalidate`/`clear` keeps a fetch that started earlier from being written back. `create`, `delete*` and `prompt_versions.update` invalidate the prompt name.

**Background work** (`src/background.rs`): two Tokio tasks spawned on first use (skipped with a log outside a runtime): the score queue behind `client.score` (batched, sent as idempotent because every queued score has an ID) and the media upload queue (`media_upload_concurrency` uploads at once). Workers hold only `Arc<HttpClient>`, so they never keep the client alive; `client.flush`/`shutdown` wait for both.

**Media** (`src/media.rs`): `LangfuseMedia` (media ID = first 22 chars of the URL-safe SHA-256). Upload: `get_upload_url` → presigned PUT through `HttpClient::raw_client` without Langfuse credentials (`x-amz-checksum-sha256`, plus `x-ms-blob-type` except for `storage.googleapis.com`) → `patch`. `extract_media` is used by `create_dataset_item` and the `otel` exporter; the helpers only the exporter uses carry `cfg_attr(not(feature = "otel"), allow(dead_code))`.

**Tracing** (`src/otel/`, `opentelemetry`/`opentelemetry_sdk` 0.33, re-exported from `otel`):
- `LangfuseTracer` (`tracer.rs`) sits on a `LangfuseSpanProcessor` (`processor.rs`): `new` builds its own `SdkTracerProvider`, `with_provider` uses one the user built with a clone of the processor and leaves it running on `shutdown`. `flush`/`shutdown` export spans first, then flush the client, because the export queues media uploads.
- `Observation` (`observation.rs`) buffers attributes and writes them once at `end` (or when the last handle drops), because the SDK's `set_attribute` appends duplicates and caps attributes per span. Above `OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT` (else 128) metadata keys are dropped first, then keys by `CORE_ATTRIBUTES` priority. `in_scope` and `propagate_attributes` build their context lazily (first poll), so scopes nest in either order.
- `PropagatedState` (`propagation.rs`) lives in the context; the processor applies `inherited_attributes` in `on_start`. Experiment attributes and `skills_available` never go into baggage.
- The processor filters in `on_end` (`should_export_span`, foreign-project spans) and hands spans to the export task with a non-blocking `try_send`. The exporter (`exporter.rs`) batches, then: de-duplicate attributes (last wins) → `upload_media` → `mask_otel_spans` (a panic drops the batch) → the custom exporter or the Langfuse endpoint (gzip, idempotent retries). `options.rs` resolves the environment variables.
- `LangfuseTracer::score` applies the tracer's sampling, with the same decision as the spans.
- `#[observe]`: proc-macro in `macros/src/lib.rs` (rewrites the body into `async move`, with an `if false { return … }` type pin when the return type is nameable, or a closure; recognizes `impl Stream`/`impl Iterator` returns), runtime in `observe.rs` (`otel::__observe`, `#[doc(hidden)]`), starting from the current observation's tracer, else `LangfuseTracer::global()`. Its autoref dispatch means calls need one more `&` than the preferred impl's `Self`.
- `attributes.rs` holds the public attribute keys: metadata is flattened to `<prefix>.<key>` with JSON values, integers beyond 2^53 become strings. `tracing_layer.rs` is the `tracing` bridge.

**Datasets, experiments, evaluation.** `get_dataset` reads every item page and `create_dataset_item` uploads media before creating the item (`src/dataset.rs`). `src/experiment.rs` holds what experiments and batch evaluation share (evaluation types, the dyn-compatible evaluator traits, `Task`) and `catch_panic`, which wraps every task, evaluator and mapper call so a panic becomes an item or evaluator failure. `run_experiment` (`src/otel/experiment.rs`) builds the `experiment-item-run` → `experiment-item-task` / `experiment-item-evaluation` span tree; `run_batched_evaluation` (`src/batch_evaluation.rs`) loops over observation cursors with a resumable `BatchEvaluationResumeToken`.

**Python parity.** These modules port the Python SDK and must keep its behaviour: `src/skills.rs` (`_client/skills.py`), `src/usage.rs` (`_parse_usage_model`, tested with the cases of `test_parse_usage_model.py`), `src/openai.rs` (`langfuse/openai.py`), and experiment item IDs plus `python_isoformat`/`python_float_repr` (Python's `json.dumps`, `isoformat` and float `repr`).

**AI Gateway** (`src/gateway.rs`, no feature; https://langfuse.com/docs/gateway/overview): not part of `api/openapi.yml`, and gateway keys cannot call the public API. `GatewayClient` (`from_env`: `LANGFUSE_GATEWAY_KEY`, `LANGFUSE_GATEWAY_HOST`, else derived from `LANGFUSE_BASE_URL`) proxies the providers' native APIs under `{host}/openai/v1` and `{host}/anthropic/v1` with `Authorization: Bearer <gateway key>`. It has its own `reqwest::Client` (10 min timeout) and retry loop on `RetryPolicy`: only GET is idempotent, so POST model calls are retried on 408/429/503 and connection failures only. Streams use a private incremental SSE parser over `Response::chunk` (no reqwest `stream` feature). `GatewayHeaders` encodes the `langfuse-*` request headers within the gateway's size limits; with `otel` it reads the current span context, computed before a call's first await; with `openai`, `GatewayOpenAIConfig` implements `async_openai::config::Config` and builds headers per request. Not in the blocking client.

**Integrations.** `Bedrock` (`src/bedrock.rs`) and `OpenAI` (`src/openai.rs`) wrap the SDK clients with a `LangfuseTracer`; per-call handles take `with_*` options, and traced streams accumulate chunks and finish on end, error or drop. Bedrock SDK types have no `Serialize` and are converted by hand; the OpenAI wrapper's `to_json` round-trips through text so `f32` values stay short.

**Blocking client** (`src/blocking/`): `mod.rs` holds the runtime, `blocking_methods!`, the pagination wrappers and the non-generated methods. `api.rs` is generated by `scripts/generate_blocking.py`; never edit it by hand.

**`YamlTool`** (`src/tools/`): standalone YAML utility on `serde_norway`, not `serde_yml` (RUSTSEC-2025-0068). Not part of the API client.

## Conventions

- All API methods return `Result<T, LangfuseError>`. API paths start with `/api/public/`; pass every interpolated path parameter through `encode_path_segment` (`folder/name` → `folder%2Fname`).
- Use the HTTP helper that matches the spec response: a JSON body needs a JSON helper, 204 a no-content helper (`request` deserializes every 2xx body).
- Every list method returning `Paginated` / `CursorPaginated` gets a `<method>_pages` companion (params by value, `paginate_pages` / `paginate_cursor`), its params get `impl_page_query!` / `impl_cursor_query!`, and the route is added to the wiring tests in `tests/pagination_tests.rs`.
- A new resource needs: model structs in `src/models/` and a client in `src/api/`, both registered in their `mod.rs`; a field and its construction in `LangfuseClient` (`src/lib.rs`); the field in `client_with_custom_config` (`tests/config_tests.rs`); a `tests/api_<resource>_tests.rs` wiremock file; serde tests for its enums (`tests/serde_new_resources_tests.rs`, or `tests/serde_unstable_tests.rs` for unstable APIs). Unstable sub-clients and their test files are behind `unstable`.
- Public option structs (`GetPromptOptions`, `ScoreQueueOptions`, `TracingOptions`, `PropagatedAttributes`, `GetDatasetOptions`, `ExperimentOptions`, `BatchEvaluationOptions`, `RequestOptions`) are `#[non_exhaustive]` with public fields and one `with_<field>` builder per field; add the builder with every new field. Outside the crate (tests, examples, README, doc tests) build them with `Default::default().with_*(..)`, or `ExperimentOptions::new(name)` / `BatchEvaluationOptions::new(mapper)`; the same goes for `Evaluation`, `EvaluatorInput` and `LocalExperimentItem` (constructors plus `with_*`).
- After adding or changing a public method of an API client, run `python3 scripts/generate_blocking.py` (`tests/blocking_tests.rs` fails otherwise).
- Both published crates inherit `version` from `[workspace.package]` in the root `Cargo.toml`; bump it together with the `=x.y.z` pin on `langfuse-rust-sdk-macros` in `[workspace.dependencies]`.
- Record user-visible changes in `CHANGELOG.md`.
