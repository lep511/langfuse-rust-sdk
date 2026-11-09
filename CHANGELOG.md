# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate uses
[Semantic Versioning](https://semver.org/) (pre-1.0: minor versions may break).

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
