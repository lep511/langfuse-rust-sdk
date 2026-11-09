# Plan: Langfuse Rust SDK — Full OpenAPI Conformance

> **Status (2026-11-09): all phases DONE.** 120/120 operations implemented (99 stable, 21 behind
> the `unstable` feature). Beyond this plan, the work also fixed spec mismatches found during
> the audit (`ScoreV3`, batch `scores.create`, `newLabels`, project `retention`, delete
> responses, `Model.startDate`, chat placeholders, `UpdateSkillRequest`, path encoding, OTel
> ingestion header) and stopped retrying 409. See `CHANGELOG.md` (0.2.0) for the full list.

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

## Phase 2: Complete partially-implemented clients — DONE

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

## Phase 3–8: New resource groups — DONE

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

## Phase 9: Unstable APIs (behind `unstable` feature flag) — DONE

Gate these behind `#[cfg(feature = "unstable")]` in module declarations and `LangfuseClient` fields, since the upstream API marks them as unstable.

| Step | Resource | Ops |
|------|----------|-----|
| 9a | UnstableDashboardWidgets | 5 |
| 9b | UnstableDashboards | 8 (includes placements sub-resource) |
| 9c | UnstableTopics | 8 (facets, topics, rules, summaries, assignments) |

**6 new source files**, all `#[cfg(feature = "unstable")]`.

---

## Phase 10: wiremock integration tests — DONE

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

## Phase 11: Cargo verification pipeline — DONE

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

---

## Appendix: operation coverage audit (2026-11-09)

Every `openapi.yml` operation, the SDK method that implements it, and a wiremock test that
exercises it (generated by matching each test's mocked method + path against the spec paths).

| # | Tag | operationId | Method + path | SDK method | wiremock test |
|---|---|---|---|---|---|
| 1 | AnnotationQueues | `annotationQueues_listQueues` | `GET /api/public/annotation-queues` | `annotation_queues.list_queues` | `api_annotation_queues_tests::list_queues` |
| 2 | AnnotationQueues | `annotationQueues_createQueue` | `POST /api/public/annotation-queues` | `annotation_queues.create_queue` | `api_annotation_queues_tests::create_queue` |
| 3 | AnnotationQueues | `annotationQueues_getQueue` | `GET /api/public/annotation-queues/{queueId}` | `annotation_queues.get_queue` | `api_annotation_queues_tests::get_queue` |
| 4 | AnnotationQueues | `annotationQueues_deleteQueueAssignment` | `DELETE /api/public/annotation-queues/{queueId}/assignments` | `annotation_queues.delete_assignment` | `api_annotation_queues_tests::delete_assignment_sends_body` |
| 5 | AnnotationQueues | `annotationQueues_createQueueAssignment` | `POST /api/public/annotation-queues/{queueId}/assignments` | `annotation_queues.create_assignment` | `api_annotation_queues_tests::create_assignment` |
| 6 | AnnotationQueues | `annotationQueues_listQueueItems` | `GET /api/public/annotation-queues/{queueId}/items` | `annotation_queues.list_items` | `api_annotation_queues_tests::list_items_filters_by_status` |
| 7 | AnnotationQueues | `annotationQueues_createQueueItem` | `POST /api/public/annotation-queues/{queueId}/items` | `annotation_queues.create_item` | `api_annotation_queues_tests::create_item` |
| 8 | AnnotationQueues | `annotationQueues_deleteQueueItem` | `DELETE /api/public/annotation-queues/{queueId}/items/{itemId}` | `annotation_queues.delete_item` | `api_annotation_queues_tests::delete_item` |
| 9 | AnnotationQueues | `annotationQueues_getQueueItem` | `GET /api/public/annotation-queues/{queueId}/items/{itemId}` | `annotation_queues.get_item` | `api_annotation_queues_tests::get_item` |
| 10 | AnnotationQueues | `annotationQueues_updateQueueItem` | `PATCH /api/public/annotation-queues/{queueId}/items/{itemId}` | `annotation_queues.update_item` | `api_annotation_queues_tests::update_item_status` |
| 11 | BlobStorageIntegrations | `blobStorageIntegrations_getBlobStorageIntegrations` | `GET /api/public/integrations/blob-storage` | `blob_storage_integrations.list` | `api_blob_storage_tests::list_integrations` |
| 12 | BlobStorageIntegrations | `blobStorageIntegrations_upsertBlobStorageIntegration` | `PUT /api/public/integrations/blob-storage` | `blob_storage_integrations.upsert` | `api_blob_storage_tests::upsert_integration_uses_put` |
| 13 | BlobStorageIntegrations | `blobStorageIntegrations_deleteBlobStorageIntegration` | `DELETE /api/public/integrations/blob-storage/{id}` | `blob_storage_integrations.delete` | `api_blob_storage_tests::delete_integration` |
| 14 | BlobStorageIntegrations | `blobStorageIntegrations_getBlobStorageIntegrationStatus` | `GET /api/public/integrations/blob-storage/{id}` | `blob_storage_integrations.get_status` | `api_blob_storage_tests::get_status` |
| 15 | Comments | `comments_get` | `GET /api/public/comments` | `comments.list` | `api_comments_tests::list_comments_with_filters` |
| 16 | Comments | `comments_create` | `POST /api/public/comments` | `comments.create` | `api_comments_tests::create_comment` |
| 17 | Comments | `comments_get-by-id` | `GET /api/public/comments/{commentId}` | `comments.get` | `api_comments_tests::get_comment_by_id` |
| 18 | DatasetItems | `datasetItems_list` | `GET /api/public/dataset-items` | `dataset_items.list` | `api_dataset_items_tests::list_items_with_version` |
| 19 | DatasetItems | `datasetItems_create` | `POST /api/public/dataset-items` | `dataset_items.create` | `api_dataset_items_tests::create_and_get_item` |
| 20 | DatasetItems | `datasetItems_delete` | `DELETE /api/public/dataset-items/{id}` | `dataset_items.delete` | `api_dataset_items_tests::delete_item_returns_message` |
| 21 | DatasetItems | `datasetItems_get` | `GET /api/public/dataset-items/{id}` | `dataset_items.get` | `api_dataset_items_tests::create_and_get_item` |
| 22 | Datasets | `datasets_list` | `GET /api/public/v2/datasets` | `datasets.list` | `api_datasets_tests::list_datasets_sends_page_params` |
| 23 | Datasets | `datasets_create` | `POST /api/public/v2/datasets` | `datasets.create` | `api_datasets_tests::create_dataset_posts_body` |
| 24 | Datasets | `datasets_get` | `GET /api/public/v2/datasets/{datasetName}` | `datasets.get` | `api_datasets_tests::get_dataset_encodes_name_with_spaces_and_slashes` |
| 25 | EvaluationRules | `evaluationRules_list` | `GET /api/public/v2/evaluation-rules` | `evaluation_rules.list` | `api_evaluation_rules_tests::list_rules_with_cursor` |
| 26 | EvaluationRules | `evaluationRules_create` | `POST /api/public/v2/evaluation-rules` | `evaluation_rules.create` | `api_evaluation_rules_tests::create_rule_with_typed_filters` |
| 27 | EvaluationRules | `evaluationRules_delete` | `DELETE /api/public/v2/evaluation-rules/{evaluationRuleId}` | `evaluation_rules.delete` | `api_evaluation_rules_tests::delete_rule` |
| 28 | EvaluationRules | `evaluationRules_get` | `GET /api/public/v2/evaluation-rules/{evaluationRuleId}` | `evaluation_rules.get` | `api_evaluation_rules_tests::rate_limit_error_details` |
| 29 | EvaluationRules | `evaluationRules_update` | `PATCH /api/public/v2/evaluation-rules/{evaluationRuleId}` | `evaluation_rules.update` | `api_evaluation_rules_tests::update_rule_sends_only_set_fields` |
| 30 | Evaluators | `evaluators_list` | `GET /api/public/v2/evaluators` | `evaluators.list` | `api_evaluators_tests::list_evaluators_mixed_types` |
| 31 | Evaluators | `evaluators_create` | `POST /api/public/v2/evaluators` | `evaluators.create` | `api_evaluators_tests::create_llm_as_judge_evaluator` |
| 32 | Evaluators | `evaluators_delete` | `DELETE /api/public/v2/evaluators/{evaluatorId}` | `evaluators.delete` | `api_evaluators_tests::delete_evaluator` |
| 33 | Evaluators | `evaluators_get` | `GET /api/public/v2/evaluators/{evaluatorId}` | `evaluators.get` | `api_evaluators_tests::get_evaluator` |
| 34 | Evaluators | `evaluators_update` | `PATCH /api/public/v2/evaluators/{evaluatorId}` | `evaluators.update` | `api_evaluators_tests::update_metadata_can_clear_description` |
| 35 | Evaluators | `evaluators_listVersions` | `GET /api/public/v2/evaluators/{evaluatorId}/versions` | `evaluators.list_versions` | `api_evaluators_tests::list_versions` |
| 36 | Experiments | `experiments_listItems` | `GET /api/public/experiment-items` | `experiments.list_items` | `api_experiments_tests::list_experiment_items` |
| 37 | Experiments | `experiments_list` | `GET /api/public/experiments` | `experiments.list` | `api_experiments_tests::list_experiments_always_sends_from_start_time` |
| 38 | Feedback | `feedback_submit` | `POST /api/public/feedback` | `feedback.submit` | `api_comments_tests::submit_feedback_returns_201` |
| 39 | Health | `health_health` | `GET /api/public/health` | `health.check` | `api_health_tests::health_check_sends_auth_and_sdk_headers` |
| 40 | LlmConnections | `llmConnections_list` | `GET /api/public/llm-connections` | `llm_connections.list` | `api_llm_connections_tests::list_project_connections` |
| 41 | LlmConnections | `llmConnections_upsert` | `PUT /api/public/llm-connections` | `llm_connections.upsert` | `api_llm_connections_tests::upsert_project_connection_sends_base_url_key` |
| 42 | LlmConnections | `llmConnections_delete` | `DELETE /api/public/llm-connections/{id}` | `llm_connections.delete` | `api_llm_connections_tests::delete_project_connection` |
| 43 | LlmConnections | `llmConnections_listOrganization` | `GET /api/public/organizations/llm-connections` | `llm_connections.list_organization` | `api_llm_connections_tests::list_organization_connections` |
| 44 | LlmConnections | `llmConnections_upsertOrganization` | `PUT /api/public/organizations/llm-connections` | `llm_connections.upsert_organization` | `api_llm_connections_tests::upsert_organization_connection` |
| 45 | LlmConnections | `llmConnections_deleteOrganization` | `DELETE /api/public/organizations/llm-connections/{id}` | `llm_connections.delete_organization` | `api_llm_connections_tests::delete_organization_connection` |
| 46 | Media | `media_getUploadUrl` | `POST /api/public/media` | `media.get_upload_url` | `api_media_tests::get_upload_url_for_trace` |
| 47 | Media | `media_get` | `GET /api/public/media/{mediaId}` | `media.get` | `api_media_tests::get_media` |
| 48 | Media | `media_patch` | `PATCH /api/public/media/{mediaId}` | `media.patch` | `api_media_tests::patch_media_sends_body_and_accepts_204` |
| 49 | Metrics | `metrics_metrics` | `GET /api/public/v2/metrics` | `metrics.query` | `api_metrics_tests::query_sends_json_encoded_query_param` |
| 50 | Models | `models_list` | `GET /api/public/models` | `models.list` | `api_models_tests::list_models_parses_iso_start_date` |
| 51 | Models | `models_create` | `POST /api/public/models` | `models.create` | `api_models_tests::create_get_upsert_delete_model` |
| 52 | Models | `models_delete` | `DELETE /api/public/models/{id}` | `models.delete` | `api_models_tests::create_get_upsert_delete_model` |
| 53 | Models | `models_get` | `GET /api/public/models/{id}` | `models.get` | `api_models_tests::create_get_upsert_delete_model` |
| 54 | Models | `models_upsert` | `PUT /api/public/models/{id}` | `models.update` | `api_models_tests::create_get_upsert_delete_model` |
| 55 | Observations | `observations_getMany` | `GET /api/public/v2/observations` | `observations.list` | `api_observations_tests::environment_is_sent_as_repeated_key` |
| 56 | Opentelemetry | `opentelemetry_exportTraces` | `POST /api/public/otel/v1/traces` | `ingestion.ingest_traces` | `api_ingestion_tests::ingest_traces_sends_ingestion_version_header` |
| 57 | Organizations | `organizations_getOrganizationApiKeys` | `GET /api/public/organizations/apiKeys` | `organizations.get_api_keys` | `api_organizations_tests::get_organization_api_keys` |
| 58 | Organizations | `organizations_deleteOrganizationMembership` | `DELETE /api/public/organizations/memberships` | `organizations.delete_membership` | `api_organizations_tests::delete_organization_membership_sends_body` |
| 59 | Organizations | `organizations_getOrganizationMemberships` | `GET /api/public/organizations/memberships` | `organizations.get_memberships` | `api_organizations_tests::get_organization_memberships` |
| 60 | Organizations | `organizations_updateOrganizationMembership` | `PUT /api/public/organizations/memberships` | `organizations.update_membership` | `api_organizations_tests::update_organization_membership` |
| 61 | Organizations | `organizations_getOrganizationProjects` | `GET /api/public/organizations/projects` | `organizations.get_projects` | `api_organizations_tests::get_organization_projects` |
| 62 | Organizations | `organizations_deleteProjectMembership` | `DELETE /api/public/projects/{projectId}/memberships` | `organizations.delete_project_membership` | `api_organizations_tests::delete_project_membership_sends_body` |
| 63 | Organizations | `organizations_getProjectMemberships` | `GET /api/public/projects/{projectId}/memberships` | `organizations.get_project_memberships` | `api_organizations_tests::get_project_memberships` |
| 64 | Organizations | `organizations_updateProjectMembership` | `PUT /api/public/projects/{projectId}/memberships` | `organizations.update_project_membership` | `api_organizations_tests::update_project_membership` |
| 65 | Projects | `projects_get` | `GET /api/public/projects` | `projects.list` | `api_projects_tests::list_projects` |
| 66 | Projects | `projects_create` | `POST /api/public/projects` | `projects.create` | `api_projects_tests::create_project_sends_retention` |
| 67 | Projects | `projects_delete` | `DELETE /api/public/projects/{projectId}` | `projects.delete` | `api_projects_tests::delete_project_parses_202_response` |
| 68 | Projects | `projects_update` | `PUT /api/public/projects/{projectId}` | `projects.update` | `api_projects_tests::update_project_omits_retention_when_unset` |
| 69 | Projects | `projects_getApiKeys` | `GET /api/public/projects/{projectId}/apiKeys` | `projects.get_api_keys` | `api_projects_tests::get_api_keys` |
| 70 | Projects | `projects_createApiKey` | `POST /api/public/projects/{projectId}/apiKeys` | `projects.create_api_key` | `api_projects_tests::create_api_key` |
| 71 | Projects | `projects_deleteApiKey` | `DELETE /api/public/projects/{projectId}/apiKeys/{apiKeyId}` | `projects.delete_api_key` | `api_projects_tests::delete_api_key` |
| 72 | PromptVersion | `promptVersion_update` | `PATCH /api/public/v2/prompts/{name}/versions/{version}` | `prompt_versions.update` | `api_prompts_tests::update_prompt_version_sends_new_labels` |
| 73 | Prompts | `prompts_list` | `GET /api/public/v2/prompts` | `prompts.list` | `api_prompts_tests::list_prompts_sends_filter_and_parses_meta` |
| 74 | Prompts | `prompts_create` | `POST /api/public/v2/prompts` | `prompts.create` | `api_prompts_tests::create_chat_prompt_with_placeholder` |
| 75 | Prompts | `prompts_delete` | `DELETE /api/public/v2/prompts/{promptName}` | `prompts.delete`, `prompts.delete_with_params` | `api_prompts_tests::delete_prompt_without_params_is_204` |
| 76 | Prompts | `prompts_get` | `GET /api/public/v2/prompts/{promptName}` | `prompts.get`, `prompts.get_with_params` | `api_prompts_tests::get_prompt_in_folder_encodes_slashes` |
| 77 | Scim | `scim_getResourceTypes` | `GET /api/public/scim/ResourceTypes` | `scim.get_resource_types` | `api_scim_tests::get_resource_types_reads_capitalized_resources` |
| 78 | Scim | `scim_getSchemas` | `GET /api/public/scim/Schemas` | `scim.get_schemas` | `api_scim_tests::get_schemas` |
| 79 | Scim | `scim_getServiceProviderConfig` | `GET /api/public/scim/ServiceProviderConfig` | `scim.get_service_provider_config` | `api_scim_tests::get_service_provider_config` |
| 80 | Scim | `scim_listUsers` | `GET /api/public/scim/Users` | `scim.list_users` | `api_scim_tests::list_users_with_scim_pagination` |
| 81 | Scim | `scim_createUser` | `POST /api/public/scim/Users` | `scim.create_user` | `api_scim_tests::create_user` |
| 82 | Scim | `scim_deleteUser` | `DELETE /api/public/scim/Users/{userId}` | `scim.delete_user` | `api_scim_tests::delete_user_accepts_empty_object_and_204` |
| 83 | Scim | `scim_getUser` | `GET /api/public/scim/Users/{userId}` | `scim.get_user` | `api_scim_tests::get_user` |
| 84 | ScoreConfigs | `scoreConfigs_get` | `GET /api/public/score-configs` | `score_configs.list` | `api_score_configs_tests::list_score_configs` |
| 85 | ScoreConfigs | `scoreConfigs_create` | `POST /api/public/score-configs` | `score_configs.create` | `api_score_configs_tests::create_score_config` |
| 86 | ScoreConfigs | `scoreConfigs_get-by-id` | `GET /api/public/score-configs/{configId}` | `score_configs.get` | `api_score_configs_tests::get_score_config_by_id` |
| 87 | ScoreConfigs | `scoreConfigs_update` | `PATCH /api/public/score-configs/{configId}` | `score_configs.update` | `api_score_configs_tests::update_score_config_archives` |
| 88 | Scores | `scores_create` | `POST /api/public/scores` | `scores.create` | `api_scores_tests::create_scores_posts_array_and_parses_202` |
| 89 | Scores | `scores_delete` | `DELETE /api/public/scores/{scoreId}` | `scores.delete` | `api_scores_tests::delete_score_accepts_204` |
| 90 | Scores | `scores_getMany` | `GET /api/public/v3/scores` | `scores.list` | `api_scores_tests::list_scores_sends_all_query_params_in_camel_case` |
| 91 | Skills | `skills_list` | `GET /api/public/skills` | `skills.list` | `api_skills_tests::list_skills_parses_pagination_with_has_next_page` |
| 92 | Skills | `skills_createVersion` | `POST /api/public/skills` | `skills.create_version` | `api_skills_tests::create_version_with_content_and_reference` |
| 93 | Skills | `skills_getFileContents` | `GET /api/public/skills/files/content` | `skills.get_file_contents` | `api_skills_tests::get_file_contents_joins_and_encodes_hashes` |
| 94 | Skills | `skills_get` | `GET /api/public/skills/{skillName}` | `skills.get` | `api_skills_tests::get_skill_by_version` |
| 95 | Skills | `skills_update` | `PATCH /api/public/skills/{skillName}` | `skills.update` | `api_skills_tests::update_skill_sends_required_tags` |
| 96 | Skills | `skills_deleteVersion` | `DELETE /api/public/skills/{skillName}/versions/{skillVersion}` | `skills.delete_version` | `api_skills_tests::delete_version_returns_deleted_flag` |
| 97 | Skills | `skills_setLabels` | `PATCH /api/public/skills/{skillName}/versions/{skillVersion}` | `skills.set_version_labels` | `api_skills_tests::set_version_labels` |
| 98 | Trace | `trace_deleteMultiple` | `DELETE /api/public/traces` | `traces.delete_batch` | `api_traces_tests::delete_batch_sends_trace_ids_and_parses_response` |
| 99 | Trace | `trace_delete` | `DELETE /api/public/traces/{traceId}` | `traces.delete` | `api_traces_tests::delete_trace_uses_delete_method` |
| 100 | UnstableDashboardWidgets | `unstable_dashboardWidgets_list` | `GET /api/public/unstable/dashboard-widgets` | `unstable_dashboard_widgets.list` | `api_unstable_dashboard_widgets_tests::list_widgets_parses_legacy_traces_view` |
| 101 | UnstableDashboardWidgets | `unstable_dashboardWidgets_create` | `POST /api/public/unstable/dashboard-widgets` | `unstable_dashboard_widgets.create` | `api_unstable_dashboard_widgets_tests::create_widget_uses_snake_case_chart_config_keys` |
| 102 | UnstableDashboardWidgets | `unstable_dashboardWidgets_delete` | `DELETE /api/public/unstable/dashboard-widgets/{widgetId}` | `unstable_dashboard_widgets.delete` | `api_unstable_dashboard_widgets_tests::delete_widget` |
| 103 | UnstableDashboardWidgets | `unstable_dashboardWidgets_get` | `GET /api/public/unstable/dashboard-widgets/{widgetId}` | `unstable_dashboard_widgets.get` | `api_unstable_dashboard_widgets_tests::get_widget` |
| 104 | UnstableDashboardWidgets | `unstable_dashboardWidgets_update` | `PATCH /api/public/unstable/dashboard-widgets/{widgetId}` | `unstable_dashboard_widgets.update` | `api_unstable_dashboard_widgets_tests::update_widget_sends_only_set_fields` |
| 105 | UnstableDashboards | `unstable_dashboards_list` | `GET /api/public/unstable/dashboards` | `unstable_dashboards.list` | `api_unstable_dashboards_tests::list_dashboards_parses_both_placement_types` |
| 106 | UnstableDashboards | `unstable_dashboards_create` | `POST /api/public/unstable/dashboards` | `unstable_dashboards.create` | `api_unstable_dashboards_tests::create_dashboard` |
| 107 | UnstableDashboards | `unstable_dashboards_delete` | `DELETE /api/public/unstable/dashboards/{dashboardId}` | `unstable_dashboards.delete` | `api_unstable_dashboards_tests::get_update_delete_dashboard` |
| 108 | UnstableDashboards | `unstable_dashboards_get` | `GET /api/public/unstable/dashboards/{dashboardId}` | `unstable_dashboards.get` | `api_unstable_dashboards_tests::get_update_delete_dashboard` |
| 109 | UnstableDashboards | `unstable_dashboards_update` | `PATCH /api/public/unstable/dashboards/{dashboardId}` | `unstable_dashboards.update` | `api_unstable_dashboards_tests::get_update_delete_dashboard` |
| 110 | UnstableDashboards | `unstable_dashboards_addPlacement` | `POST /api/public/unstable/dashboards/{dashboardId}/placements` | `unstable_dashboards.add_placement` | `api_unstable_dashboards_tests::add_placement_with_server_defaults` |
| 111 | UnstableDashboards | `unstable_dashboards_deletePlacement` | `DELETE /api/public/unstable/dashboards/{dashboardId}/placements/{placementId}` | `unstable_dashboards.delete_placement` | `api_unstable_dashboards_tests::update_and_delete_placement` |
| 112 | UnstableDashboards | `unstable_dashboards_updatePlacement` | `PATCH /api/public/unstable/dashboards/{dashboardId}/placements/{placementId}` | `unstable_dashboards.update_placement` | `api_unstable_dashboards_tests::update_and_delete_placement` |
| 113 | UnstableTopics | `unstable_topics_listAssignments` | `GET /api/public/unstable/topic-assignments` | `unstable_topics.list_assignments` | `api_unstable_topics_tests::list_assignments_last_page` |
| 114 | UnstableTopics | `unstable_topics_listFacets` | `GET /api/public/unstable/topic-facets` | `unstable_topics.list_facets` | `api_unstable_topics_tests::list_facets_with_filter` |
| 115 | UnstableTopics | `unstable_topics_getFacet` | `GET /api/public/unstable/topic-facets/{facetId}` | `unstable_topics.get_facet` | `api_unstable_topics_tests::get_facet` |
| 116 | UnstableTopics | `unstable_topics_listRules` | `GET /api/public/unstable/topic-rules` | `unstable_topics.list_rules` | `api_unstable_topics_tests::list_and_get_rules` |
| 117 | UnstableTopics | `unstable_topics_getRule` | `GET /api/public/unstable/topic-rules/{ruleId}` | `unstable_topics.get_rule` | `api_unstable_topics_tests::list_and_get_rules` |
| 118 | UnstableTopics | `unstable_topics_listSummaries` | `GET /api/public/unstable/topic-summaries` | `unstable_topics.list_summaries` | `api_unstable_topics_tests::list_summaries_with_cursor` |
| 119 | UnstableTopics | `unstable_topics_list` | `GET /api/public/unstable/topics` | `unstable_topics.list` | `api_unstable_topics_tests::list_topics_with_window_and_order` |
| 120 | UnstableTopics | `unstable_topics_get` | `GET /api/public/unstable/topics/{topicId}` | `unstable_topics.get` | `api_unstable_topics_tests::get_topic_flattens_topic_fields` |
