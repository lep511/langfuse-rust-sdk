# Plan: Langfuse Rust SDK — Full OpenAPI Conformance

## Context

The SDK currently implements **~35 of 120 operations** from `openapi.yml`. Fourteen entire API resource groups are missing, several existing clients have incomplete query parameters, there's a critical HTTP-method bug in `TracesClient::delete` (uses GET instead of DELETE), and the error enum is missing variants for 409/412/503. There are zero HTTP-mock integration tests — only unit/serde tests exist. This plan brings the SDK to full spec conformance, fixes all bugs, and adds wiremock-based test coverage.

---

## Phase 0: Foundation (git + dev deps) — DONE

1. `git init`, commit current codebase as baseline
2. Create branch `fix/openapi-conformance`
3. Add `wiremock = "0.6"` to `[dev-dependencies]` in `Cargo.toml`
4. Add `[features] unstable = []` to `Cargo.toml` (for Phase 9)
5. Run baseline: `cargo check`, `cargo clippy --all-targets`, `cargo test`

---

## Phase 1: Bug fixes + error handling improvements — DONE

### 1a — Fix `TracesClient::delete` HTTP method bug
- **`src/api/traces.rs:14`**: Change `self.http.get(...)` → `self.http.delete_with_response(...)` 
- **`src/http.rs`**: Add `delete_with_response<T: DeserializeOwned>` method

### 1b — Add missing error variants (409, 412, 503)
- **`src/error.rs`**: Add `#[non_exhaustive]` to `LangfuseError`
- Add variants: `Conflict { message, body }` (409), `PreconditionFailed { message, body }` (412), `ServiceUnavailable { message, body }` (503)
- Update `map_response_error` match arms — insert 409, 412, 503 before the `500..=599` catch-all

### 1c — Add missing `HttpClient` methods needed by later phases
- **`src/http.rs`**: Add three methods following existing patterns:
  - `delete_with_response<T>` — DELETE with JSON response (for traces, evaluators, annotation queues)
  - `patch_no_content<B>` — PATCH returning `()` (for media patch, 204 response)
  - `delete_no_content_with_query<Q>` — DELETE with query params, no body response (for prompts delete with label/version)

---

## Phase 2: Complete partially-implemented clients — TODO

Each step adds missing methods/params to existing clients. All non-breaking (adding optional fields or new methods).

| Step | Client | Change | Files |
|------|--------|--------|-------|
| 2a | ScoreConfigs | Add `update()` PATCH method + `UpdateScoreConfigRequest` model | `src/api/score_configs.rs`, `src/models/score_configs.rs` |
| 2b | Scores | Add missing query params to `ListScoresParams` (id, session_id, experiment_id, value, value_min, value_max, author_user_id, queue_id, fields) | `src/api/scores.rs` (or wherever ListScoresParams lives) |
| 2c | Observations | Add missing query params (fields, expand_metadata, parse_io_as_json, is_root_observation, filter) | `src/api/observations.rs` |
| 2d | DatasetItems | Add `version` query param to `ListDatasetItemsParams` | `src/api/dataset_items.rs` |
| 2e | Prompts | Add `resolve` to `GetPromptParams`; add `delete_with_params()` accepting label/version query params | `src/api/prompts.rs` |
| 2f | Skills | Add `get_file_contents(sha256_hashes)` + response model | `src/api/skills.rs`, `src/models/skills.rs` |
| 2g | Media | Add `patch()` (PATCH, 204) + `get_upload_url()` (POST) + request/response models | `src/api/media.rs`, `src/models/media.rs` |
| 2h | Projects | Add `get_api_keys()`, `create_api_key()`, `delete_api_key()` + models | `src/api/projects.rs`, `src/models/projects.rs` |

---

## Phase 3–8: New resource groups — TODO

Each new group follows the same pattern:
1. Create `src/models/<resource>.rs` — request/response structs with serde derives
2. Create `src/api/<resource>.rs` — client struct with `Arc<HttpClient>`, async methods
3. Add `pub mod <resource>;` to `src/models/mod.rs` and `src/api/mod.rs`
4. Add field + construction to `LangfuseClient` in `src/lib.rs`
5. Add any new enums to `src/models/enums.rs`

| Phase | Resource | Ops | Notes |
|-------|----------|-----|-------|
| 3 | Comments | 3 | Simple CRUD, good template |
| 4 | Feedback | 1 | Single POST, returns 201 |
| 5a | AnnotationQueues | 10 | Nested resources (queues → items, assignments) |
| 5b | BlobStorageIntegrations | 4 | GET/PUT/DELETE + status check |
| 6a | EvaluationRules | 5 | Cursor pagination, PublicApiError (429/500/409) |
| 6b | Evaluators | 6 | Tagged union (LlmAsJudge/Code), listVersions |
| 6c | Experiments | 2 | Required `fromStartTime` param, complex query params |
| 7a | LlmConnections | 6 | Project + org level (two sets of 3 endpoints) |
| 7b | Metrics | 1 | JSON query string param |
| 7c | Organizations | 8 | Memberships at org + project level, list projects/apiKeys |
| 8 | Scim | 7 | SCIM protocol types (ServiceProviderConfig, Users, etc.) |

**New files per group**: 2 source files (`src/api/` + `src/models/`). Approximately **22 new source files** total.

---

## Phase 9: Unstable APIs (behind `unstable` feature flag) — TODO

Gate these behind `#[cfg(feature = "unstable")]` in module declarations and `LangfuseClient` fields, since the upstream API marks them as unstable.

| Step | Resource | Ops |
|------|----------|-----|
| 9a | UnstableDashboardWidgets | 5 |
| 9b | UnstableDashboards | 8 (includes placements sub-resource) |
| 9c | UnstableTopics | 8 (facets, topics, rules, summaries, assignments) |

**6 new source files**, all `#[cfg(feature = "unstable")]`.

---

## Phase 10: wiremock integration tests — TODO

### 10a — Test helper
Create `tests/common/mod.rs` with:
- `test_client()` → `(LangfuseClient, MockServer)` pointing at mock server
- Helper to mount standard JSON response mocks

### 10b — Test files (one per resource group)
Each test verifies: correct HTTP method, path, query params, auth headers, request body serialization, response deserialization, and error handling.

Priority test files:
- `tests/api_traces_tests.rs` — **critical**: proves DELETE bug fix
- `tests/api_error_tests.rs` — tests 409/412/503 error variants, non-JSON body, empty body, 429 with Retry-After
- `tests/api_health_tests.rs`, `api_scores_tests.rs`, `api_prompts_tests.rs`, etc.
- One test file per new resource group

### 10c — Extend serde tests
Add round-trip tests to `tests/serde_tests.rs` for all new model types.

---

## Phase 11: Cargo verification pipeline — TODO

Run in order, fix any failures:
1. `cargo fmt --all --check` (format if needed)
2. `cargo check --all-targets --all-features`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo build --all-features` + `cargo build --no-default-features`
5. `cargo test --all-features` + `cargo test --no-default-features`
6. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
7. `cargo build --examples`
8. Ecosystem tools if available: `cargo audit`, `cargo deny`, `cargo machete`, `cargo outdated`, `cargo hack check --feature-powerset --no-dev-deps`, `cargo semver-checks`, `cargo publish --dry-run`

---

## Breaking changes (summary)

1. **`#[non_exhaustive]` on `LangfuseError`** — callers with exhaustive match arms break. This is intentional to allow future variant additions without semver bumps.
2. **New sub-client fields on `LangfuseClient`** — anyone destructuring the struct breaks. Add `#[non_exhaustive]` here too.
3. **Prompts `delete()`** — keep existing signature, add `delete_with_params()` as new method (non-breaking).

---

## Verification

- Every phase ends with `cargo check` + `cargo test`
- Final pass criteria: fmt/check/clippy/test/doc all green with zero warnings
- Operation coverage target: 120/120 implemented, every documented error code mapped
- Test count target: ~75 existing + ~100+ new wiremock/serde tests

---

## Key OpenAPI schema notes (extracted from openapi.yml)

### Enums needed
- `AnnotationQueueStatus`: PENDING, COMPLETED
- `AnnotationQueueObjectType`: TRACE, OBSERVATION, SESSION
- `CommentObjectType`: TRACE, OBSERVATION, SESSION, PROMPT
- `FeedbackTargetType`: skill, mcp-tool, cli, docs, public-api, other
- `LlmAdapter`: anthropic, openai, azure, bedrock, google-vertex-ai, google-ai-studio
- `MembershipRole`: OWNER, ADMIN, MEMBER, VIEWER
- `MediaContentType`: 45+ MIME types
- `TopicSummaryState`: pending, complete, not_applicable, insufficient_input
- `TopicAssignmentStatus`: assigned, outlier
- `TopicsOrderBy`: traceCount.desc, traceCount.asc, name.asc, name.desc
- Various widget/dashboard enums for unstable APIs

### Pagination styles
- Page-based: `utilsMetaResponse` (page, limit, totalItems, totalPages) — used by most list endpoints
- Cursor-based: `utilsCursorMetaResponse` (cursor nullable) — used by experiments, evaluation rules

### Tagged unions
- `ScoreV3`: dispatch on `dataType` (existing)
- `Prompt`: dispatch on `type` (existing)  
- `Evaluator`: dispatch on `type` (LlmAsJudgeEvaluator | CodeEvaluator)
- Dashboard placements: dispatch on `type` (widget | preset)
