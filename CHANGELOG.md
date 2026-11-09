# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate uses
[Semantic Versioning](https://semver.org/) (pre-1.0: minor versions may break).

## [0.4.0] - 2026-11-09

Prompt management: client-side cache and compilation.

### Breaking changes

- `BasePrompt` gained `is_fallback` (never sent by the API); struct literals
  need it.
- `prompts.create`, `prompts.delete`, `prompts.delete_with_params` and
  `prompt_versions.update` now also invalidate the cached versions of the
  prompt (see `get_cached` below).

### Added

- **Prompt compilation**: `Prompt::variables()`, `Prompt::compile(&variables)`
  and `Prompt::compile_with_placeholders(&variables, placeholders)`, returning
  `CompiledPrompt::{Text, Chat}`. `{{variable}}` placeholders (whitespace
  ignored) take their values from anything that serializes to a JSON object;
  unknown variables are left as-is, `null` becomes empty and non-strings are
  inserted as compact JSON. Chat placeholders are replaced by lists of
  `ChatMessage` (whose contents are compiled too); unresolved ones stay in
  place. Accessors `Prompt::{base, name, version, config, is_fallback}`.
- **Prompt cache**: `PromptsClient::get_cached(name, &GetPromptOptions)`
  returns an `Arc<Prompt>` from an in-memory cache shared by client clones:
  - TTL per call (`cache_ttl`) or default from `ConfigBuilder::prompt_cache_ttl`
    / `LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS` (60 s; zero bypasses the cache);
  - expired prompts are served immediately while one background task per
    prompt and selector refreshes them; a `404` evicts the prompt;
  - `PromptSelector::{Label, Version}` (default: label `production`);
  - `PromptFallback::{Text, Chat}`, returned with `is_fallback() == true` when
    the prompt is neither cached nor fetchable (never cached);
  - per-call `max_retries` and `fetch_timeout`;
  - fetches overtaken by an invalidation are not written back;
  - `PromptsClient::clear_cache()`.

## [0.3.0] - 2026-11-09

Configuration from the environment, pagination streams, ID helpers and client
utilities.

### Breaking changes

- **OTel ingestion models**: `OtelAnyValue` gained `array_value` and derives
  `Default`; `OtelSpan` gained `trace_state`, `events` and `links`. Struct
  literals need the new fields: use `..Default::default()` for `OtelAnyValue`
  and `..OtelSpan::new(..)` for `OtelSpan`, or the new constructors.
- `ConfigBuilder::header` no longer drops an invalid header name or value
  silently: `build()` now returns `LangfuseError::Config` for it.

### Added

- `ClientConfig::from_env()` / `ConfigBuilder::from_env()` read
  `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL` (with the
  deprecated `LANGFUSE_HOST` as fallback) and `LANGFUSE_TIMEOUT`. All examples
  use it.
- `ConfigBuilder::http_client(reqwest::Client)` sends requests through a custom
  client (proxies, TLS, shared pool). An explicit `timeout()` is applied per
  request; otherwise the client's own timeout is kept.
- `LangfuseClient` and every sub-client implement `Clone` (clones share the
  connection pool).
- `LangfuseClient::auth_check()` verifies the API keys and returns their project.
- `LangfuseClient::trace_url(trace_id)` builds
  `{base_url}/project/{project_id}/traces/{trace_id}`, caching the project ID.
- `ids::create_trace_id(seed)` / `ids::create_observation_id(seed)`: random W3C
  IDs, or deterministic ones derived from `SHA-256(seed)`.
- Lazy pagination streams: a `*_pages` method next to every list method that
  returns `Paginated` or `CursorPaginated` (21 methods, e.g.
  `observations.list_pages`), returning `pagination::Pages`; `Pages::items()`
  flattens it into items. Generic `pagination::paginate_pages` /
  `paginate_cursor` and the `PageQuery` / `CursorQuery` traits cover custom
  use.
- OTLP helpers: `OtelKeyValue::{new, string, int, double, bool, strings, json}`,
  the matching `OtelAnyValue` constructors plus `OtelAnyValue::array`,
  `OtelSpan::new`, `OtelSpanEvent` and `OtelSpanLink`. `arrayValue` was
  verified against the Langfuse API (e.g. `langfuse.trace.tags`).

### Fixed

- A base URL with a path (`https://example.com/langfuse`) is now kept as the
  prefix of every request; before, requests went to `https://example.com/api/...`.

## [0.2.2] - 2026-11-09

### Added

- Comprehensive documentation for all public items in the core modules (`lib`,
  `config`, `error`, `http`, `pagination`, `retry`, `tools`), including a
  crate-level doc comment with quick-start, configuration and error-handling
  examples.
- Module-level `//!` doc comments on every `api/` and `models/` sub-module so
  docs.rs shows a description next to each item in the module listing.

### Fixed

- Eliminated all 1,599 `missing_docs` compiler warnings. The `api` and `models`
  modules now use `#![allow(missing_docs)]` since their OpenAPI-derived types
  have self-documenting field names.
- Updated all test and example imports from `langfuse_sdk` to `langfuse_rust_sdk`
  to match the crate rename introduced in v0.2.0.
- Fixed outdated template references in `PUBLISHING.md`.

## [0.2.0] - 2026-11-09

The SDK now covers all 120 operations of `openapi.yml` (99 stable, 21 behind the
`unstable` feature). Several existing types were changed to match the spec;
most of them failed at runtime against the real API before.

### Breaking changes

- **Scores**
  - `ScoreV3` follows the v3 spec: `value` is `f64` (numeric), `bool` (boolean) or
    `String` (categorical, text, correction). `string_value` was removed.
  - `BaseScore` gained `project_id` and `subject: Option<ScoreSubjectV3>`
    (`trace`/`observation`/`session`/`experiment`); `trace_id`, `session_id`,
    `observation_id` and `dataset_run_id` were removed (use `subject`).
  - `ScoresClient::create` takes a slice (`&[CreateScoreRequest]`, the endpoint is a
    batch) and returns `CreateScoresResponse` (`Accepted` for 202, `BatchResults`
    for 207). `CreateScoreResponse` was removed.
  - `CreateScoreRequest` gained `source` and `timestamp`; use `CreateScoreRequest::new`.
- **Prompts**
  - Chat prompts hold `Vec<ChatPromptMessage>` (`Message` or `Placeholder`) instead
    of `Vec<ChatMessage>`, in both `Prompt::Chat` and `CreatePromptRequest::Chat`.
  - `PromptMeta.version: Option<i64>` became `versions: Vec<i64>`; `type` is now
    `PromptType`; added `last_updated_at` and `last_config`.
  - `UpdatePromptVersionRequest` is `{ new_labels: Vec<String> }` (wire name `newLabels`).
  - `PromptVersionClient::get` was removed (the endpoint does not exist).
- **Deletes return the documented body**: `dataset_items.delete`
  (`DeleteDatasetItemResponse`), `projects.delete` (`ProjectDeletionResponse`, HTTP 202),
  `skills.delete_version` (`DeleteSkillVersionResponse`), `traces.delete_batch`
  (`DeleteTraceResponse`).
- **Projects**: `CreateProjectRequest.retention` (required) and
  `UpdateProjectRequest.retention` replace `retention_days`, which the server did not accept.
- **Skills**
  - `UpdateSkillRequest` is `{ tags: Vec<String> }` (required; `description` is not in the spec).
  - `SetSkillVersionLabelsRequest.labels` is `Vec<String>` (required).
  - `skills.list` returns `SkillListResponse` (meta includes `has_next_page`);
    `SkillMeta` gained the latest/production version fields.
  - `CreateSkillVersionRequest.files` is `Vec<SkillVersionFileCreateInput>`
    (inline content or a reference by hash).
  - `GetSkillParams` gained `version`.
- **Models**: `Model.start_date` and `CreateModelRequest.start_date` are
  `DateTime<Utc>` (the API sends ISO date-times; `NaiveDate` failed to parse).
  `Model` gained `created_at`, `is_langfuse_managed`, tokenizer fields, `prices` and
  `pricing_tiers`; `CreateModelRequest` gained `pricing_tiers`, `tokenizer_id`,
  `tokenizer_config` (use `CreateModelRequest::new`).
- **Observations**: `ListObservationsParams.environment` is `Option<Vec<String>>`,
  sent as a repeated key (`environment=a&environment=b`). The struct now serializes
  as a list of `(key, value)` pairs.
- **Media**: `GetMediaResponse.url_expiry` is a `String` (the spec gives no format).
- **OTel**: `OtelSpan` gained `kind` and `OtelInstrumentationScope` gained `attributes`.
- **Enums**: every enum deserialized from responses is `#[non_exhaustive]` and has an
  `Unknown` variant used for values this version does not know. `Unknown` cannot be
  serialized. Exhaustive `match`es need a wildcard arm.
- **`LangfuseClient` is `#[non_exhaustive]`** so new sub-clients can be added.
- **Retries**: HTTP 409 is no longer retried (`RetryPolicy::should_retry` covers
  408, 429 and 5xx). A conflict now returns `LangfuseError::Conflict` immediately.
- **Path parameters are percent-encoded**, so names with `/` (prompt folders) or
  spaces reach the right route. Callers that pre-encoded names must stop doing so.
- **`YamlTool`**: `Value` and `YamlError::Parse` now come from `serde_norway`
  instead of `serde_yml` (same API shape). YAML output differs in one way: strings
  such as `yes`/`no`/`on`/`off` are no longer quoted; they still parse back as strings.
- **Error variants**: `LangfuseError` is
  `#[non_exhaustive]` with new `Conflict` (409), `PreconditionFailed` (412) and
  `ServiceUnavailable` (503) variants; 503 is no longer `ServerError`.

### Security

- Replaced `serde_yml` 0.0.12 with `serde_norway` 0.9 (a maintained fork of `serde_yaml`).
  `serde_yml` and its `libyml` dependency are unsound and unmaintained
  ([RUSTSEC-2025-0068](https://rustsec.org/advisories/RUSTSEC-2025-0068),
  [RUSTSEC-2025-0067](https://rustsec.org/advisories/RUSTSEC-2025-0067)).
  `cargo audit` now reports no advisories.

### Added

- New sub-clients: `comments`, `feedback`, `annotation_queues`,
  `blob_storage_integrations`, `evaluation_rules`, `evaluators`, `experiments`,
  `llm_connections`, `metrics`, `organizations`, `scim`.
- Feature `unstable`: `unstable_dashboard_widgets`, `unstable_dashboards`,
  `unstable_topics`.
- Missing operations of existing clients: `score_configs.update`,
  `prompts.delete_with_params`, `skills.get_file_contents`, `media.patch`,
  `media.get_upload_url`, `projects.get_api_keys` / `create_api_key` / `delete_api_key`.
- Missing query parameters: scores list (`id`, `session_id`, `experiment_id`, `value`,
  `value_min`, `value_max`, `author_user_id`, `queue_id`, `fields`), observations list
  (`fields`, `expand_metadata`, `parse_io_as_json`, `is_root_observation`, `filter`),
  dataset items `version`, prompts `resolve` and `filter`.
- `DatasetItem.version` and `media_references`.
- `ingestion.ingest_traces` sends `x-langfuse-ingestion-version: 4`, as the API requires.
- `LangfuseError::body()`, plus `PublicApiError::from_error` and
  `UnstablePublicApiError::from_error` to read structured error bodies.
- `HttpClient` helpers: `delete_with_response`, `delete_with_body_response`,
  `patch_no_content`, `delete_no_content_with_query`, `post_no_response_with_headers`.
- Constructors for requests with many optional fields (`CreateScoreRequest::new`,
  `CreateModelRequest::new`, `GetMediaUploadUrlRequest::new`, ...).
- 10 new examples: `metrics_query`, `experiments`, `comments`, `annotation_queues`,
  `evaluators`, `prompt_labels`, `llm_connections`, `organization_admin`, `scim_users`
  and `dashboards` (requires `--features unstable`).
- 149 wiremock HTTP tests (`tests/api_*_tests.rs`, shared helpers in `tests/common`)
  and serde tests for every new model.

## [0.1.0]

Initial version.
