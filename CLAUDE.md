# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust SDK for the Langfuse LLM observability API. Async-only, built on `reqwest` + `tokio`. Targets Rust 1.99+ (edition 2024). The SDK implements all 120 operations of `openapi.yml`; the 21 `/api/public/unstable/*` operations are behind the `unstable` Cargo feature.

## Build & Test Commands

```bash
cargo build                                # Build the library
cargo test                                 # All tests without unstable APIs (412)
cargo test --all-features                  # Including unstable APIs (439)
cargo test --test api_scores_tests         # One wiremock test file
cargo test --test serde_tests              # Serialization tests of the original models
cargo test --lib                           # In-crate unit tests (retry, http, config from_env, ids, prompt templates, prompt_cache, yaml_tool)
cargo clippy --all-targets --all-features -- -D warnings
cargo run --example health_check           # Example against the real API (needs .env)
cargo run --example dashboards --features unstable  # Examples of unstable APIs need the feature
```

Tests never touch the network. HTTP behaviour is tested with `wiremock` in `tests/api_*_tests.rs`; shared helpers live in `tests/common/mod.rs` (`test_client()`, `test_client_with_retries(n)`, `json(status, body)`, `single_request`, `query_pairs`, `query_value`). Client helpers (`Clone`, `auth_check`, `trace_url`, custom `reqwest::Client`) are tested in `tests/client_tests.rs` the `*_pages` streams in `tests/pagination_tests.rs` (which consumes them with the `futures` dev-dependency), prompt compilation in `tests/prompt_compile_tests.rs` and `get_cached` in `tests/prompt_cache_tests.rs` (timing-based: short TTLs and delayed mocks, polled with bounded waits). Test files for unstable APIs start with `#![cfg(feature = "unstable")]`. Examples in `examples/` hit the real Langfuse API; each doc comment says whether it writes data and how it cleans up. Examples using unstable APIs are declared in `Cargo.toml` with `required-features = ["unstable"]`. Examples build their client with `ClientConfig::from_env()` and need `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL` via `.env` or environment.

## Architecture

**Entry point:** `LangfuseClient` in `src/lib.rs` (`#[non_exhaustive]`, `Clone`) — constructed from a `ClientConfig`, it holds one sub-client per resource group (all `Clone`), each wrapping a shared `Arc<HttpClient>`, plus the helpers `auth_check()` and `trace_url()` (project ID cached in an `Arc<OnceLock>` shared by clones). Unstable sub-clients (`unstable_dashboard_widgets`, `unstable_dashboards`, `unstable_topics`) are `#[cfg(feature = "unstable")]`.

**Core layers:**

- `src/config.rs` — Builder pattern (`ClientConfig::builder(pk, sk).base_url(...).build()`) or `ClientConfig::from_env()` / `ConfigBuilder::from_env()` (`LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL`/`LANGFUSE_HOST`, `LANGFUSE_TIMEOUT`; tested in-crate through `from_lookup`, never by mutating the process environment). Produces HTTP Basic Auth header, SDK identification headers (`x-langfuse-sdk-name`, `x-langfuse-sdk-version`, `x-langfuse-public-key`). The base URL always ends with `/` and `HttpClient` joins request paths relative to it, so a base path prefix is kept. `http_client(reqwest::Client)` injects a custom client (explicit `timeout()` then applies per request); an invalid `header()` makes `build()` fail.
- `src/http.rs` — `HttpClient` wraps `reqwest::Client`. One private `execute` loop handles auth, default and per-request headers, query, JSON body and retries. Public helpers:
  - JSON response: `get`, `get_with_query`, `post`, `put`, `patch`, `delete_with_response` (no body), `delete_with_body_response` (JSON body).
  - Body ignored (204 and similar): `post_no_response`, `post_no_response_with_headers`, `patch_no_content`, `delete_no_content`, `delete_no_content_with_query`, `delete_with_body`.
  - `encode_path_segment` (crate-private): percent-encode every interpolated path parameter (`folder/name` → `folder%2Fname`). Always use it when building paths.
- `src/retry.rs` — `RetryPolicy` with exponential backoff + jitter. Honors `Retry-After`. Retries depend on idempotency (`HttpClient` decides from the method, GET/HEAD/PUT/DELETE/OPTIONS, unless `RequestOptions::idempotent` overrides it, as OTLP ingestion does): 408, 429, 503 and connection failures are retried for every request; other 5xx, timeouts and dropped connections only for idempotent ones (`RetryPolicy::is_retryable_for`, `http::retry_transport_error`). 409 is never retried. The public `should_retry(status, attempt)` keeps the idempotent, status-only rule.
- `src/error.rs` — `LangfuseError` (`#[non_exhaustive]`) maps status codes to variants: 400, 401, 403, 404, 405, 409 `Conflict`, 412 `PreconditionFailed`, 429 `RateLimited`, 503 `ServiceUnavailable`, other 5xx `ServerError`. `body()` returns the parsed JSON error body.
- `src/pagination.rs` — `Paginated<T>` (page-based, `PageMeta`) and `CursorPaginated<T>` (`CursorMeta`), plus `PageParams` / `CursorParams`. Lazy streams: `paginate_pages` / `paginate_cursor` return `Pages<'a, P>` (boxed, `Send`, `Unpin`); `Pages::items()` returns `Items`. Params implement `PageQuery` / `CursorQuery` through the crate-private `impl_page_query!` / `impl_cursor_query!` macros, invoked next to each params struct.
- `src/ids.rs` — `create_trace_id(seed)` / `create_observation_id(seed)`: random W3C IDs, or `SHA-256(seed)` truncated to 16/8 bytes.
- `src/prompt_cache.rs` (crate-private) — cache behind `PromptsClient::get_cached`: `Mutex<State>` with entries (`Arc<Prompt>` + expiry; expired entries stay readable), the keys being refreshed (claimed through a `RefreshGuard` dropped at the end of the background `tokio` task) and a generation counter bumped by `invalidate`/`clear`, so a fetch that started before an invalidation is not written back. Shared by `PromptsClient` and `PromptVersionClient`; `create`, `delete*` and `prompt_versions.update` invalidate the prompt name. Per-call `max_retries` / `fetch_timeout` go through the crate-private `RequestOptions` of `HttpClient` (`get_with_query_options`).
- Prompt compilation lives in `src/models/prompts.rs`: `Prompt::variables`, `compile`, `compile_with_placeholders` (`CompiledPrompt<ChatPromptMessage>`), `compile_with_json_placeholders` (`CompiledPrompt<serde_json::Value>`, keeps extra message fields such as `tool_calls`) and the private `{{variable}}` template engine with its unit tests.

**API clients** (`src/api/`): One file per resource group. Each struct holds `Arc<HttpClient>`; query-param structs live next to the client. API paths use the `/api/public/` prefix.

**Models** (`src/models/`): Request/response structs with serde derives. Key patterns:
- Response structs: `Deserialize` + `#[serde(rename_all = "camelCase")]`; nullable or non-required spec fields are `Option`.
- Request structs: `Serialize`, `skip_serializing_if = "Option::is_none"` on optional fields; structs with many optional fields have a `new(required...)` constructor for use with `..Struct::new(..)`.
- Enums deserialized from responses are declared with the `wire_enum!` macro (`src/models/macros.rs`): `#[non_exhaustive]` plus an `Unknown` variant (`#[serde(other, skip_serializing)]`). Request-only enums are plain `Serialize` enums without `Unknown`. Tagged unions stay strict (no `Unknown`).
- Tagged unions use `#[serde(tag = "...")]` with `#[serde(flatten)]` for shared base fields: `ScoreV3` (`dataType`), `ScoreSubjectV3` (`kind`), `Prompt` (`type`), `Evaluator`/`EvaluatorVersion` (`type`), `EvaluatorOutputDefinition` (`dataType`), `EvaluationRuleFilter` (`type`), `DashboardPlacement` (`type`).
- Constant fields (e.g. `"type": "placeholder"`, `"operator": "in"`) use `#[serde(tag = "...", rename = "...")]` on the struct.
- Wire names that `camelCase` cannot produce need explicit renames: `baseURL`, SCIM `Resources`, `row_limit`, `show_value_labels`, `every_20_minutes`, filter operators with spaces, `orderBy` values with dots.
- JSON-in-query parameters (`filter`, metrics `query`) are `String`s (or typed and serialized with `serde_json::to_string`, as in `MetricsClient::query`). `ListObservationsParams` serializes as key/value pairs so `environment` can repeat.
- Shared types: `evaluation_common.rs` (evaluators + evaluation rules: `Creator`, variable mappings, `PublicApiError`, `EvaluationRuleReadFilter`), `unstable_common.rs` (`UnstablePublicApiError`, `DashboardWidgetFilter`).

**Tools** (`src/tools/`): `YamlTool` — standalone YAML utility (read/write/merge/validate/frontmatter extraction) using `serde_norway` (maintained `serde_yaml` fork; `serde_yml` was dropped for RUSTSEC-2025-0068). Not part of the API client.

## Conventions

- All API methods return `Result<T, LangfuseError>`.
- Every list method returning `Paginated` / `CursorPaginated` gets a `<method>_pages` companion (params by value, `paginate_pages` / `paginate_cursor`), its params get `impl_page_query!` / `impl_cursor_query!`, and the route is added to the wiring tests in `tests/pagination_tests.rs`.
- Use the HTTP helper that matches the spec response: a JSON body needs a JSON helper; 204 needs a no-content helper (`request` deserializes every 2xx body).
- Adding a new resource requires: model structs in `src/models/`, API client in `src/api/`, both registered in their `mod.rs`, a field and its construction in `LangfuseClient` (`src/lib.rs`), the field in `client_with_custom_config` (`tests/config_tests.rs`), a `tests/api_<resource>_tests.rs` wiremock file and serde tests for its enums (`tests/serde_new_resources_tests.rs`, or `tests/serde_unstable_tests.rs` for unstable APIs).
- Record user-visible changes in `CHANGELOG.md`.
