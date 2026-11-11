# Examples

31 runnable programs that show the SDK against a real Langfuse project. Each
one is a single file in the
[`examples/`](https://github.com/lep511/langfuse-rust-sdk/tree/main/examples)
directory of the repository, with a doc comment that says what it does, what
it needs and what data it leaves behind.

Two more examples, `rig_tracing` and `bedrock_tracing`, live in
[`integration-tests/examples/`](https://github.com/lep511/langfuse-rust-sdk/tree/main/integration-tests/examples)
because they need rig or the AWS SDK; run them with
`cargo run -p langfuse-rust-sdk-integration-tests --example <name> --features <feature>`.

- [Setup](#setup)
- [Conventions](#conventions)
- [Overview](#overview)
- [Rate limits to keep in mind](#rate-limits-to-keep-in-mind)
- [Getting started](#getting-started): [`health_check`](#health_check), [`error_handling`](#error_handling), [`blocking`](#blocking)
- [Tracing](#tracing): [`otel_tracing`](#otel_tracing), [`observe`](#observe), [`otel_ingestion`](#otel_ingestion), [`otel_export_options`](#otel_export_options), [`media`](#media), [`openai_tracing`](#openai_tracing)
- [Reading data](#reading-data): [`observations`](#observations), [`metrics_query`](#metrics_query), [`comments`](#comments)
- [Scores and human review](#scores-and-human-review): [`create_score`](#create_score), [`list_scores`](#list_scores), [`score_configs`](#score_configs), [`annotation_queues`](#annotation_queues)
- [Datasets, experiments and evaluation](#datasets-experiments-and-evaluation): [`manage_datasets`](#manage_datasets), [`run_experiment`](#run_experiment), [`experiments`](#experiments), [`batch_evaluation`](#batch_evaluation), [`evaluators`](#evaluators)
- [Prompts and skills](#prompts-and-skills): [`prompts`](#prompts), [`prompt_labels`](#prompt_labels), [`skills`](#skills)
- [Project configuration](#project-configuration): [`manage_models`](#manage_models), [`llm_connections`](#llm_connections), [`dashboards`](#dashboards)
- [Organization administration](#organization-administration): [`organization_admin`](#organization_admin), [`scim_users`](#scim_users)
- [AI Gateway](#ai-gateway): [`gateway`](#gateway), [`gateway_openai`](#gateway_openai)
- [Verification](#verification)

## Setup

The examples read their configuration from the environment and load a `.env`
file from the working directory first (variables that are already set win).
Run them from the repository root:

```sh
cat > .env <<EOF
LANGFUSE_PUBLIC_KEY=pk-lf-...
LANGFUSE_SECRET_KEY=sk-lf-...
LANGFUSE_BASE_URL=https://cloud.langfuse.com
EOF

cargo run --example health_check
cargo run --example otel_tracing --features otel
```

| Variable | Used by | Meaning |
|---|---|---|
| `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY` | all | API keys of a project (an organization-scoped key for the two [organization examples](#organization-administration)) |
| `LANGFUSE_BASE_URL` | all | API base URL. Default `https://cloud.langfuse.com` (Langfuse Cloud EU); the US region is `https://us.cloud.langfuse.com`; self-hosted: your own URL |
| `LANGFUSE_GATEWAY_KEY`, `LANGFUSE_GATEWAY_HOST` | `gateway`, `gateway_openai` | Key and host of the Langfuse AI Gateway, e.g. `https://gateway.cloud.langfuse.com` (EU) or `https://gateway.us.cloud.langfuse.com` (US) |
| `OPENAI_API_KEY` | `openai_tracing` | OpenAI key (`OPENAI_BASE_URL` selects another OpenAI-compatible server) |

Use a project meant for testing: most examples write data, and some of it
cannot be deleted through the API.

Cargo refuses to run an example whose feature is not enabled; the command in
each section includes the `--features` flag. `cargo build --examples
--all-features` builds all of them at once.

## Conventions

- Every doc comment starts with what the example shows, followed by
  **Read-only** or **Writes data** (what is written, whether it is kept and
  how to remove it), **Needs** (feature, key scope, extra variables) and a link
  to the Langfuse documentation.
- Named objects that stay in the project start with `rust-sdk` (the two
  score configs of `score_configs` are the exception), and most traces carry
  the tag `rust-sdk`, so they are easy to find in the UI.
- Examples that keep their data do so on purpose, so that you can open the
  result in the UI; they print the URL.
- Ingestion is asynchronous. Examples that read back what they just wrote
  poll for it, which is why a few of them run for up to a minute.
- Observations queries always send a lower time bound: without
  `fromStartTime` the v2 Observations API searches from 2020-01-01.

## Overview

| Example | Feature | Writes data | Left behind | Runtime |
|---|---|---|---|---|
| [`health_check`](#health_check) | – | no | – | < 1 s |
| [`error_handling`](#error_handling) | – | no | – | < 1 s |
| [`blocking`](#blocking) | `blocking` | no | – | < 1 s |
| [`otel_tracing`](#otel_tracing) | `otel` | yes | 1 trace, 1 score | < 1 s |
| [`observe`](#observe) | `macros` | yes | 1 trace | < 1 s |
| [`otel_ingestion`](#otel_ingestion) | – | yes | 1 trace | < 1 s |
| [`otel_export_options`](#otel_export_options) | `otel` | yes | nothing (trace deleted) | ~20 s |
| [`media`](#media) | `otel` | yes | 1 trace, 2 media files | ~25 s |
| [`openai_tracing`](#openai_tracing) | `openai` | yes | 1 trace | a few s |
| [`observations`](#observations) | – | no | – | < 1 s |
| [`metrics_query`](#metrics_query) | – | no | – | ~1 s |
| [`comments`](#comments) | – | yes | 1 comment | ~3 s |
| [`create_score`](#create_score) | – | yes | 2 scores | < 1 s |
| [`list_scores`](#list_scores) | – | no | – | < 1 s |
| [`score_configs`](#score_configs) | – | yes | 2 score configs per run | < 1 s |
| [`annotation_queues`](#annotation_queues) | – | yes | 1 queue (reused), maybe 1 score config | ~1 s |
| [`manage_datasets`](#manage_datasets) | – | yes | 1 dataset with 3 items (reused) | ~1 s |
| [`run_experiment`](#run_experiment) | `otel` | yes | nothing (traces and scores deleted) | ~1 min |
| [`experiments`](#experiments) | – | no | – | ~1 s |
| [`batch_evaluation`](#batch_evaluation) | – | yes | nothing (trace and scores deleted) | ~1 min |
| [`evaluators`](#evaluators) | – | yes | nothing (rule and evaluator deleted) | ~1 s |
| [`prompts`](#prompts) | – | yes | 2 prompts, 1 new version each per run | < 1 s |
| [`prompt_labels`](#prompt_labels) | – | yes | nothing (prompt deleted) | ~1 s |
| [`skills`](#skills) | – | yes | 1 skill, 1 new version per run | < 1 s |
| [`manage_models`](#manage_models) | – | yes | nothing (model deleted) | < 1 s |
| [`llm_connections`](#llm_connections) | – | optional | nothing (connection deleted) | < 1 s |
| [`dashboards`](#dashboards) | `unstable` | yes | nothing (dashboard and widgets deleted) | pause + ~2 s |
| [`organization_admin`](#organization_admin) | – | no | – | < 1 s |
| [`scim_users`](#scim_users) | – | no | – | < 1 s |
| [`gateway`](#gateway) | `otel` | yes | 1 trace, 2 gateway generations | ~5 s |
| [`gateway_openai`](#gateway_openai) | `openai` | yes | 1 trace, 1 gateway generation | ~2 s |

## Rate limits to keep in mind

Langfuse Cloud limits requests per organization and per group of endpoints
([limits](https://langfuse.com/faq/all/api-limits)); self-hosted instances
have no hard limits. When a limit is hit the API answers `429` with a
`Retry-After` header, and the SDK waits and retries. Three groups are small
enough to matter when running the examples repeatedly on the Hobby plan:

| Group | Hobby | Core | Pro and above | Requests per run |
|---|---|---|---|---|
| Trace data deletion (deleting traces and scores) | 50 / day | 200 / day | 1,000 / day | `run_experiment` 11, `batch_evaluation` 9, `otel_export_options` 1 |
| Metrics API v2 | 100 / day | 100 / hour | 500 / hour | `metrics_query` 2 |
| General API (everything not listed elsewhere) | 30 / min | 100 / min | 1,000 / min | `dashboards` 14; `batch_evaluation`, `run_experiment`, `media` and `otel_export_options` send one request every 2–3 s while they wait for ingestion |

Deleting a trace also deletes its observations and scores, asynchronously
(usually within 15 minutes,
[data deletion](https://langfuse.com/docs/administration/data-deletion)).
`run_experiment` and `batch_evaluation` nevertheless delete their scores one by
one, so that the project is clean as soon as they return; that is what makes
them the two expensive ones.

---

## Getting started

### `health_check`

The smallest useful program: checks that the server is up, verifies the API
keys and prints the project and organization they belong to. Run it first
with a new setup.

```sh
cargo run --example health_check
```

- **Source:** [`examples/health_check.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/health_check.rs)
- **Data:** read-only.
- **Calls:** `client.health.check()` → `GET /api/public/health` (needs no
  credentials); `client.auth_check()` → `GET /api/public/projects`.
- **Docs:** [Public API](https://langfuse.com/docs/api-and-data-platform/features/public-api)

```text
Langfuse server is healthy!
  Status:  OK
  Version: 4.78.0
API keys are valid for project 'test'
  Project:      <project id>
  Organization: test
```

### `error_handling`

Every API method returns `Result<T, LangfuseError>`, with one variant per HTTP
status. The example provokes five failures and handles each: an invalid base
URL (`Config`, no request sent), a missing skill and a missing prompt
(`NotFound`), invalid keys (`Unauthorized`) and a missing dataset handled as a
"get or create" fallback.

```sh
cargo run --example error_handling
```

- **Source:** [`examples/error_handling.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/error_handling.rs)
- **Data:** read-only (every request is a `GET` that fails).
- **Calls:** `skills.get`, `prompts.get`, `auth_check` with a second client
  built from invalid keys, `datasets.get`.
- **Good to know:** `LangfuseError` is `#[non_exhaustive]`, so keep a
  catch-all arm. 408, 429, 503 and connection failures are retried by the SDK
  before the error is returned.

```text
=== Config Error ===
  Config error caught: Invalid base URL '://bad': relative URL without a base

=== Not Found ===
  Skill not found (expected): Skill version not found

=== Prompt Not Found ===
  Prompt not found (expected): Prompt not found: 'nonexistent-prompt-xyz' with label 'production'

=== Unauthorized ===
  Unauthorized (expected): Invalid credentials. Confirm that you've configured the correct host.

=== Graceful Fallback ===
  Dataset not found — could create it here
```

### `blocking`

The blocking client from a plain `fn main()`: no async runtime, no `.await`.
Checks health and keys, lists prompts through a *view* of the client with
per-request options (10 s timeout, no retries, an extra header and a hook that
logs every response), and iterates over datasets with a lazy page iterator.

```sh
cargo run --example blocking --features blocking
```

- **Source:** [`examples/blocking.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/blocking.rs)
- **Data:** read-only.
- **Needs:** the `blocking` feature. The blocking client runs the async SDK on
  a runtime of its own and panics when called from async code.
- **Calls:** `health.check`, `auth_check`, `prompts.list` →
  `GET /api/public/v2/prompts`, `datasets.list_pages` →
  `GET /api/public/v2/datasets`.

```text
Langfuse 4.78.0 is OK
Project: test (<project id>)
  GET /api/public/v2/prompts -> 200 OK (attempt 1)
2 prompts in total; first ones:
  rust-sdk-assistant
  rust-sdk-greeting
Datasets:
  rust-sdk-example
```

---

## Tracing

### `otel_tracing`

The standard way to instrument an application. One chat request becomes a
trace: an `agent` observation with a `retriever`, a `generation` (model,
parameters, usage) and an `event` (level `WARNING`) nested under it. User,
session, tags and trace name are propagated to every observation, and the
generation gets a score.

```sh
cargo run --example otel_tracing --features otel
```

- **Source:** [`examples/otel_tracing.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/otel_tracing.rs)
- **Data:** writes one trace (four observations) and one score, kept. Delete
  the trace in the UI or with `client.traces.delete(trace_id)`.
- **Needs:** the `otel` feature.
- **Optional variables:** `LANGFUSE_TRACING_ENVIRONMENT`, `LANGFUSE_RELEASE`,
  `LANGFUSE_SAMPLE_RATE`, `LANGFUSE_TRACING_ENABLED` (read by
  `TracingOptions::from_env`).
- **Calls:** `client.tracer`, `tracer.start_observation`,
  `Observation::in_scope`, `propagate_attributes`, `observation.score`,
  `tracer.shutdown`. Spans go to `POST /api/public/otel/v1/traces`, the score
  to `POST /api/public/scores`.
- **Docs:** [Instrumentation](https://langfuse.com/docs/observability/sdk/instrumentation),
  [Observation types](https://langfuse.com/docs/observability/features/observation-types)

```text
Traced '<observation id>' (trace <trace id>)
URL: https://cloud.langfuse.com/project/<project id>/traces/<trace id>
```

### `observe`

The same kind of trace with almost no tracing code: `#[observe]` turns each
call of a function into an observation, with the arguments as input and the
return value as output. Shows an agent, a retriever, a tool that returns an
error (recorded with level `ERROR`), a generation that returns a stream (kept
open until the stream ends), a trace ID chosen up front, and the list of
skills available to the agent attached to the trace.

```sh
cargo run --example observe --features macros
```

- **Source:** [`examples/observe.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/observe.rs)
- **Data:** writes one trace (four observations), kept.
- **Needs:** the `macros` feature (includes `otel`).
- **Optional variables:** `LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED=false`
  stops recording arguments and results. Skills are discovered in `./skills`
  if that directory exists.
- **Calls:** `#[observe]`, `Observation::current`, `tracer.install_global`,
  `in_trace`, `propagate_attributes`, `discover_skills`,
  `TracingOptions::with_skills`.
- **Docs:** [Instrumentation](https://langfuse.com/docs/observability/sdk/instrumentation),
  [Skill tracing](https://langfuse.com/docs/skill-management/tracing)

```text
Answer: Your order is on its way: shipped
URL: https://cloud.langfuse.com/project/<project id>/traces/<trace id>
```

### `otel_ingestion`

The low-level path: builds one OTLP span by hand (resource, scope, span,
attributes) and posts it to Langfuse's OpenTelemetry endpoint. No `otel`
feature and no OpenTelemetry SDK. Useful to forward spans produced elsewhere,
and to see which attributes Langfuse reads: `langfuse.observation.type`,
`langfuse.observation.input`, `langfuse.trace.name`, `langfuse.trace.tags`,
`gen_ai.request.model`, `gen_ai.request.*` (model parameters) and
`gen_ai.usage.*`.

```sh
cargo run --example otel_ingestion
```

- **Source:** [`examples/otel_ingestion.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/otel_ingestion.rs)
- **Data:** writes one trace with one generation, kept.
- **Calls:** `client.ingestion.ingest_traces` →
  `POST /api/public/otel/v1/traces` (OTLP/HTTP JSON, sent with
  `x-langfuse-ingestion-version: 4`); `ids::create_trace_id`,
  `ids::create_observation_id`; `client.trace_url`.
- **Good to know:** export a span once. Langfuse does not deduplicate span IDs,
  so sending the same span again creates a second observation.
- **Docs:** [OpenTelemetry](https://langfuse.com/integrations/native/opentelemetry),
  [Migrate custom ingestion to v4](https://langfuse.com/integrations/native/opentelemetry/migration-to-v4)

```text
Ingested OTel trace: <trace id>
  Span: llm-generation (2.5s, 150 in / 320 out tokens, tags rust-sdk + example)
  URL:  https://cloud.langfuse.com/project/<project id>/traces/<trace id>
```

### `otel_export_options`

Three export options, verified end to end: gzip-compressed exports (the
default), your own `SdkTracerProvider` that feeds Langfuse *and* another
exporter, and `mask_otel_spans`, which redacts span attributes before they
leave the process, including those of spans created by other instrumentation.
The example then reads the trace back through the API, checks that the secret
did not arrive, and deletes the trace. It exits with an error if the check
fails.

```sh
cargo run --example otel_export_options --features otel
```

- **Source:** [`examples/otel_export_options.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/otel_export_options.rs)
- **Data:** writes one trace with three observations and deletes it (one
  delete request). `KEEP_TRACE=1` keeps it and prints its URL.
- **Needs:** the `otel` feature.
- **Calls:** `LangfuseSpanProcessor::new`, `LangfuseTracer::with_provider`,
  `TracingOptions::with_compression`, `TracingOptions::with_mask_otel_spans`,
  `observations.list` → `GET /api/public/v2/observations`, `traces.delete` →
  `DELETE /api/public/traces/{traceId}`.
- **Docs:** [Masking](https://langfuse.com/docs/observability/features/masking),
  [Advanced features](https://langfuse.com/docs/observability/sdk/advanced-features)

```text
Tracing (spans go to Langfuse and to the other backend):
  [other backend] account-lookup (<span id>)
  [other backend] POST /v1/chat (<span id>)
  [other backend] export-options-demo (<span id>)
Waiting for Langfuse to store trace <trace id>...
  POST /v1/chat: metadata {"attributes.gen_ai.system":"openai","attributes.security.redacted":true,...}
  account-lookup: metadata {"api_key":"[redacted]","region":"eu",...}
  export-options-demo: metadata {...}
✓ gzip export stored, secrets redacted
Trace deleted
```

### `media`

Multi-modal tracing. Attaches a PNG (inside an OpenAI-style message) and a
text file (in metadata) to a generation. The exporter uploads both to
Langfuse's object storage and stores reference strings in their place. The
example then reads the observation back and resolves the image reference into
a data URI again.

```sh
cargo run --example media --features otel
```

- **Source:** [`examples/media.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/media.rs)
- **Data:** writes one trace, kept, and two media files. Media is identified
  by project, content type and SHA-256 of the content, so later runs upload
  nothing new.
- **Needs:** the `otel` feature.
- **Calls:** `LangfuseMedia::from_data_uri`, `LangfuseMedia::from_bytes`,
  `tracer.flush` (exports, then waits for the uploads),
  `observations.list`, `client.media.resolve_references`. Uploads use
  `POST /api/public/media`, a presigned `PUT`, and
  `PATCH /api/public/media/{mediaId}`.
- **Docs:** [Multi-modality](https://langfuse.com/docs/observability/features/multi-modality)

```text
Traced generation <observation id> with an image and a text file
URL: https://cloud.langfuse.com/project/<project id>/traces/<trace id>
Stored input: [{"content":[{"text":"What color is this pixel?","type":"text"},{"image_url":{"url":"@@@langfuseMedia:type=image/png|id=<media id>|source=base64_data_uri@@@"},"type":"image_url"}],"role":"user"}]
Resolved image: data:image/png;base64,iVBORw0KGgoAAAANSU...
```

### `openai_tracing`

Traces calls made with `async-openai`, like the OpenAI integration of the
Python SDK: a chat completion, a streamed chat completion and an embedding
request. The chat calls are recorded as generations, the embedding as an
`embedding` observation, all under one agent observation, with input, output,
model, model parameters and usage.

```sh
OPENAI_API_KEY=sk-... cargo run --example openai_tracing --features openai
```

- **Source:** [`examples/openai_tracing.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/openai_tracing.rs)
- **Data:** writes one trace (four observations), kept. The three OpenAI calls
  are billed by OpenAI.
- **Needs:** the `openai` feature and `OPENAI_API_KEY`.
- **Optional variables:** `OPENAI_BASE_URL` (another OpenAI-compatible server,
  which must also serve `text-embedding-3-small`), `OPENAI_MODEL` (chat model,
  default `gpt-4o-mini`).
- **Calls:** `openai::OpenAI::new`, `openai.chat().with_name(..).create(..)`,
  `.create_stream(..)`, `openai.embeddings().create(..)`.
- **Good to know:** OpenAI reports token usage on a stream only when the
  request sets `stream_options.include_usage`; the streamed generation of this
  example therefore has no usage. Do not wrap a client that talks to the AI
  Gateway, which records the call itself (see [`gateway_openai`](#gateway_openai)).
- **Docs:** [OpenAI integration](https://langfuse.com/integrations/model-providers/openai-py)

```text
Answers: <answer> / <streamed answer>
URL: https://cloud.langfuse.com/project/<project id>/traces/<trace id>
```

---

## Reading data

### `observations`

Prints the ten most recent observations of the last 24 hours as a table, using
the v2 Observations API: the real-time read path that replaces the legacy
trace, session and observation endpoints.

```sh
cargo run --example observations
```

- **Source:** [`examples/observations.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/observations.rs)
- **Data:** read-only.
- **Calls:** `observations.list` → `GET /api/public/v2/observations` with
  `fields=core,basic,model,metrics`.
- **Good to know:** only the `core` and `basic` field groups are returned by
  default; the model needs `model`, the latency `metrics`, input and output
  `io`. Pagination is cursor-based (`meta.cursor`, or `observations.list_pages`).
- **Docs:** [Public API](https://langfuse.com/docs/api-and-data-platform/features/public-api)

```text
Recent observations of the last 24 hours (10 returned):
TYPE         LEVEL          NAME                      MODEL                 LATENCY
-------------------------------------------------------------------------------------
EMBEDDING    Default        OpenAI-embedding          text-embedding-3-sm      0.1s
GENERATION   Default        answer                    gpt-4o-mini              0.0s
AGENT        Default        travel-assistant          -                        0.1s
...

Next cursor: eyJsYXN0U3RhcnRUaW1lVG8iOiIyMDI2LTExLTEx
```

### `metrics_query`

Two aggregate queries with the v2 Metrics API, the API behind custom
dashboards: generations per model with count, tokens and cost over the last 7
days (grouped by a dimension, filtered, ordered by cost), and a daily time
series of observation count and p95 latency (milliseconds).

```sh
cargo run --example metrics_query
```

- **Source:** [`examples/metrics_query.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/metrics_query.rs)
- **Data:** read-only. Two requests of the Metrics API rate limit per run.
- **Calls:** `metrics.query` → `GET /api/public/v2/metrics?query=<JSON>`.
- **Good to know:** views are `observations`, `scores-numeric`,
  `scores-categorical` and `scores-boolean`. Result columns are named
  `<aggregation>_<measure>`; the time bucket is `time_dimension`.
- **Docs:** [Metrics API](https://langfuse.com/docs/metrics/features/metrics-api)

```text
=== Generations per model (4 rows) ===
  {"count_count":4,"providedModelName":"us.anthropic.claude-sonnet-5-5","sum_totalCost":0.001688,"sum_totalTokens":268}
  {"count_count":6,"providedModelName":"gpt-4o-mini","sum_totalCost":0.0000438,"sum_totalTokens":184}
  ...

=== Observations per day (7 rows) ===
  {"count_count":0,"p95_latency":null,"time_dimension":"2026-11-09"}
  {"count_count":78,"p95_latency":2508.9999999999995,"time_dimension":"2026-11-10"}
```

### `comments`

Attaches a markdown comment to a trace, fetches it by ID and lists the
comments of that trace. Comments can be attached to traces, observations,
sessions and prompts.

```sh
cargo run --example comments
```

- **Source:** [`examples/comments.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/comments.rs)
- **Data:** writes one comment, kept. The public API cannot delete comments;
  remove it in the UI.
- **Optional variables:** `LANGFUSE_TRACE_ID` picks the trace. Otherwise the
  example takes the trace of the most recent observation of the last 24 hours
  and, if there is none, says so and exits (run `otel_ingestion` first).
- **Calls:** `projects.list`, `observations.list`, `comments.create` →
  `POST /api/public/comments`, `comments.get`, `comments.list`.
- **Docs:** [Comments](https://langfuse.com/docs/observability/features/comments)

```text
Project test / trace <trace id>
Created comment <comment id>
Fetched: [Trace <trace id>] Reviewed from the Rust SDK: **output looks correct**.

Comments on this trace (1):
  2026-11-11 12:54 by API: Reviewed from the Rust SDK: **output looks correct**.
```

---

## Scores and human review

### `create_score`

Creates a numeric and a categorical score for a trace in one request. The
endpoint takes a batch and ingests it asynchronously: `202` means everything
was accepted, `207` returns the rejected scores with their reasons.

```sh
cargo run --example create_score
```

- **Source:** [`examples/create_score.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/create_score.rs)
- **Data:** writes two scores (`accuracy`, `sentiment`) on the placeholder
  trace ID `example-trace-id`, kept. A score may reference a trace that does
  not exist; put a real trace ID in the `TRACE_ID` constant to see the scores
  on a trace. Remove them with `client.scores.delete(score_id)`.
- **Calls:** `scores.create` → `POST /api/public/scores`.
- **Good to know:** application code usually scores through the background
  queue instead (`client.score(..)`, `observation.score(..)`, see
  [`otel_tracing`](#otel_tracing)).
- **Docs:** [Scores via API/SDK](https://langfuse.com/docs/evaluation/evaluation-methods/scores-via-sdk)

```text
Batch accepted: Accepted
```

### `list_scores`

Walks through the project's scores with the lazy cursor-pagination stream
`scores.list_pages(..).items()` and prints each one with its data type
(numeric, boolean, categorical, text or correction). Stops after 100 scores.

```sh
cargo run --example list_scores
```

- **Source:** [`examples/list_scores.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/list_scores.rs)
- **Data:** read-only.
- **Calls:** `scores.list_pages` → `GET /api/public/v3/scores`, one page of
  ten per request, requested only when the stream reaches it.
- **Good to know:** every paginated list method has a `*_pages` companion.
- **Docs:** [Scores](https://langfuse.com/docs/evaluation/scores/overview)

```text
  [1] accuracy = 0.67 (numeric)
  [2] overall = 0.20 (numeric)
  [3] exact_match = true (boolean)
  [4] sentiment = positive (categorical)
  ...

Total scores fetched: 26
```

### `score_configs`

Creates a numeric score config (range 0 to 1) and a categorical one (four
labelled values), then lists the project's configs. A score config is the
schema of a score; scores that reference one are validated against it, and
annotation queues are built from them.

```sh
cargo run --example score_configs
```

- **Source:** [`examples/score_configs.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/score_configs.rs)
- **Data:** writes two score configs **on every run** (`relevance-score`,
  `response-quality`; names need not be unique). Score configs cannot be
  deleted, only archived: in the UI or with `client.score_configs.update`.
- **Calls:** `score_configs.create` → `POST /api/public/score-configs`,
  `score_configs.list` → `GET /api/public/score-configs`.
- **Docs:** [Scores data model](https://langfuse.com/docs/evaluation/scores/data-model)

```text
Created numeric config: relevance-score (<config id>)
  Range: [0, 1]

Created categorical config: response-quality (<config id>)

All score configs: 6 of 6
  - response-quality (Categorical) — <config id>
  - relevance-score (Numeric) — <config id>
  ...
```

### `annotation_queues`

The human-review workflow through the API: reuse or create a queue, add a
trace to it, list the pending items, mark the item as completed and remove
it. Reviewers normally do the middle part in the UI.

```sh
cargo run --example annotation_queues
```

- **Source:** [`examples/annotation_queues.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/annotation_queues.rs)
- **Data:** the queue `rust-sdk-example-queue` is created on the first run
  and reused afterwards (queues cannot be deleted through the API). A queue
  needs a score config: the example uses the project's first active one, or
  creates the numeric `rust-sdk-review-quality`. The queue item is deleted at
  the end.
- **Optional variables:** `LANGFUSE_TRACE_ID` picks the trace (default: the
  most recent trace of the last 24 hours; without one the example says so and
  exits). `LANGFUSE_EXAMPLE_USER_ID` also assigns and unassigns that user as
  a reviewer.
- **Calls:** `annotation_queues.list_queues`, `create_queue`, `create_item`,
  `list_items`, `update_item`, `delete_item`, optionally `create_assignment`
  and `delete_assignment` (all under `/api/public/annotation-queues`);
  `score_configs.list`, `observations.list`.
- **Docs:** [Annotation queues](https://langfuse.com/docs/evaluation/evaluation-methods/annotation-queues)

```text
Reusing queue 'rust-sdk-example-queue'
Queued trace <trace id> as item <item id> (Pending)
Pending items in the queue: 1
Item <item id> is now Completed (completed at 2026-11-11T12:54:45.780+00:00)
Removed item: Annotation queue item deleted successfully
```

---

## Datasets, experiments and evaluation

### `manage_datasets`

Creates a dataset, upserts three question/answer items with fixed IDs,
fetches the dataset back with all its items and lists the project's datasets.

```sh
cargo run --example manage_datasets
```

- **Source:** [`examples/manage_datasets.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/manage_datasets.rs)
- **Data:** writes the dataset `rust-sdk-example` with three items, kept.
  Re-running changes nothing: dataset names are unique and items are upserted
  on their ID. The API cannot delete a dataset (only items, with
  `client.dataset_items.delete`).
- **Calls:** `datasets.create` → `POST /api/public/v2/datasets`,
  `client.create_dataset_item` → `POST /api/public/dataset-items` (uploads
  media found in the item first), `client.get_dataset` (dataset plus every
  page of items), `datasets.list`.
- **Docs:** [Datasets](https://langfuse.com/docs/evaluation/experiments/datasets)

```text
Created dataset: rust-sdk-example (<dataset id>)
  Upserted item: rust-sdk-example-0
  Upserted item: rust-sdk-example-1
  Upserted item: rust-sdk-example-2

Fetched dataset: rust-sdk-example (3 items)
...
All datasets (page 1): 1 of 1
```

### `run_experiment`

Runs an experiment with the experiment runner on local data. A toy lookup
"model" answers three capital-city questions (one wrong on purpose); two item
evaluators, a composite evaluator and a run evaluator score the result. The
example prints the report, waits until Langfuse lists the experiment, deletes
its traces and scores, and checks that the experiment is gone.

```sh
cargo run --example run_experiment --features otel
```

- **Source:** [`examples/run_experiment.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/run_experiment.rs)
- **Data:** writes three traces and ten scores, all deleted at the end (11
  delete requests). `KEEP_EXPERIMENT=1` keeps them and leaves the printed
  experiment URL valid.
- **Needs:** the `otel` feature.
- **Calls:** `tracer.run_experiment(data, task, options)`, `evaluator`,
  `composite_evaluator`, `run_evaluator`, `ExperimentOptions`,
  `experiments.list` → `GET /api/public/experiments`, `scores.list`,
  `scores.delete`, `traces.delete_batch` → `DELETE /api/public/traces`.
- **Good to know:** on a Langfuse dataset use
  `client.get_dataset(..)` and `dataset.run_experiment(&tracer, task, options)`.
  Experiments are not created through an API call: Langfuse derives them from
  the ingested traces.
- **Docs:** [Experiments via SDK](https://langfuse.com/docs/evaluation/experiments/experiments-via-sdk)

```text
3. Item 3:
   Input:    {"country":"Spain"}
   Expected: Madrid
   Actual:   Barcelona
   Scores:
     • exact_match: 0.000
       💭 Wrong capital
     • brevity: 1.000
     • overall: 0.200
...
Average Scores:
  • brevity: 1.000
  • exact_match: 0.667
  • overall: 0.733

Run Evaluations:
  • accuracy: 0.667
    💭 2 of 3 correct
...
Langfuse lists the experiment: yes

Deleted 10 scores and 3 traces.
Experiment still listed after deleting its traces: no
```

### `experiments`

Lists the experiments of the last 30 days with their scores (manual cursor
pagination, at most three pages) and shows the items of the first one: input,
output, expected output and scores.

```sh
cargo run --example experiments
```

- **Source:** [`examples/experiments.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/experiments.rs)
- **Data:** read-only. Prints a hint if there is no experiment in the window;
  `KEEP_EXPERIMENT=1 cargo run --example run_experiment --features otel`
  creates one.
- **Calls:** `experiments.list` → `GET /api/public/experiments`,
  `experiments.list_items` → `GET /api/public/experiment-items`.
  `fromStartTime` is required on both.
- **Docs:** [Experiments data model](https://langfuse.com/docs/evaluation/experiments/data-model)

```text
Experiments since 2026-10-12T12:50:03Z: 2
  - capital-cities - 2026-11-10T21:21:30.987531Z (3 items, dataset -) [accuracy=0.667]
  ...

Items of 'capital-cities - 2026-11-10T21:21:30.987531Z' (first 3):
  trace <trace id>
    input:    "{\"country\":\"Spain\"}"
    output:   "Barcelona"
    expected: "Madrid"
    scores:   brevity=1.000, exact_match=false, overall=0.200
```

### `batch_evaluation`

Scores observations that are already stored in Langfuse with
`run_batched_evaluation`: it pages through the observations matching a
filter, runs evaluators on each and writes the results back as scores. To have
data, the example first ingests a trace with three generations; one evaluator
fails on purpose for the generation with an empty answer.

```sh
cargo run --example batch_evaluation
```

- **Source:** [`examples/batch_evaluation.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/batch_evaluation.rs)
- **Data:** writes one trace and eight scores, all deleted at the end (nine
  delete requests). Takes about a minute, mostly waiting for ingestion.
- **Calls:** `client.run_batched_evaluation(options)`,
  `BatchEvaluationOptions` (mapper, evaluators, composite evaluator, filter,
  metadata, `max_items`), `ingestion.ingest_traces`, `observations.list`,
  `scores.list`, `scores.delete`, `traces.delete`.
- **Good to know:** the time window is a `startTime` condition of the filter.
  A run that stops at `max_items` or on an error returns a resume token.
- **Docs:** [Evaluation overview](https://langfuse.com/docs/evaluation/overview);
  the Python SDK's `run_batched_evaluation` is described in its
  [v4 to v5 upgrade guide](https://langfuse.com/docs/observability/sdk/upgrade-path/python-v4-to-v5)

```text
Status: Completed
Items fetched: 3
Items processed: 3
Scores created: 5
Composite scores: 3
Total scores: 8

Evaluator Performance:
  not_empty:
    Runs: 3/3 (100.0% success)
  conciseness:
    Runs: 2/3 (66.7% success)
    Failed runs: 1
...
Deleted 8 scores and the trace.
```

### `evaluators`

Sets up LLM-as-a-judge evaluation through the API: creates an evaluator (the
judge prompt and a numeric output between 0 and 1), attaches it to a
*disabled* evaluation rule for production generations with 10 % sampling,
updates the evaluator's description, lists versions and rules, and deletes
both.

```sh
cargo run --example evaluators
```

- **Source:** [`examples/evaluators.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/evaluators.rs)
- **Data:** writes an evaluator and a rule, both deleted at the end, also on
  failure. The rule is disabled, so nothing is evaluated and no model is
  called.
- **Calls:** `evaluators.create` → `POST /api/public/v2/evaluators`,
  `evaluation_rules.create` → `POST /api/public/v2/evaluation-rules`,
  `evaluators.update`, `evaluators.list_versions`, `evaluation_rules.list`,
  `evaluation_rules.delete`, `evaluators.delete`.
- **Good to know:** without a usable evaluation model (no LLM connection) the
  evaluator is created with the status `paused`, as in the output below. A
  `412 Precondition Failed` is handled too: the example prints the reason and
  stops.
- **Docs:** [LLM-as-a-judge](https://langfuse.com/docs/evaluation/evaluation-methods/llm-as-a-judge)

```text
Created evaluator rust-sdk-example-helpfulness v1 (Paused)
  prompt variables: input, output
Created rule 'rust-sdk-example production generations' (enabled: false, sampling: 0.1)
  filter: type any of ["GENERATION"]
  filter: environment any of ["production"]
Updated description: Judges answer helpfulness (Rust SDK example)
Evaluator versions: 1
Evaluation rules in the project (first page): 1
Deleted rule <rule id>
Deleted evaluator <evaluator id>
```

---

## Prompts and skills

### `prompts`

Prompt management end to end: creates a text prompt and a chat prompt (with a
message placeholder), fetches by label, fetches through the client-side cache
with a fallback, compiles variables and placeholders, and lists the project's
prompts.

```sh
cargo run --example prompts
```

- **Source:** [`examples/prompts.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/prompts.rs)
- **Data:** adds one version to `rust-sdk-greeting` and one to
  `rust-sdk-assistant` on every run, kept. Delete with
  `client.prompts.delete(name)`.
- **Calls:** `prompts.create` → `POST /api/public/v2/prompts`,
  `prompts.get_with_params` → `GET /api/public/v2/prompts/{name}`,
  `prompts.get_cached` (cache TTL 60 s by default, stale entries are served
  while a background task refreshes them), `Prompt::variables`, `compile`,
  `compile_with_placeholders`, `prompts.list`.
- **Docs:** [Prompt management](https://langfuse.com/docs/prompt-management/get-started),
  [Caching](https://langfuse.com/docs/prompt-management/features/caching),
  [Message placeholders](https://langfuse.com/docs/prompt-management/features/message-placeholders)

```text
Cached 'rust-sdk-greeting' v3 (second call from memory: true, fallback: false)
  Variables: ["name", "service"]
  Compiled:  Hello Ada, welcome to Langfuse! How can I help you today?

Compiled 'rust-sdk-assistant':
    system: You are a helpful coding assistant specializing in Rust.
    user: How do I read a file in Rust?
    assistant: Use std::fs::read_to_string.
    user: And asynchronously?
```

### `prompt_labels`

Release management with labels: creates two versions of a prompt in a folder
(`rust-sdk-examples/label-demo`), promotes version 1 to `production`, fetches
by label and by version, finds the prompt with a structured filter, deletes
version 2 and then the whole prompt.

```sh
cargo run --example prompt_labels
```

- **Source:** [`examples/prompt_labels.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/prompt_labels.rs)
- **Data:** writes a prompt and deletes it at the end, also on failure.
- **Calls:** `prompts.create`, `prompt_versions.update` →
  `PATCH /api/public/v2/prompts/{name}/versions/{version}`,
  `prompts.get_with_params`, `prompts.list` with `filter`,
  `prompts.delete_with_params`, `prompts.delete` →
  `DELETE /api/public/v2/prompts/{promptName}`.
- **Good to know:** a label points to one version of a prompt, `latest`
  always follows the newest one, and a fetch without label or version returns
  `production`.
- **Docs:** [Version control](https://langfuse.com/docs/prompt-management/features/prompt-version-control),
  [Folders](https://langfuse.com/docs/prompt-management/features/folders)

```text
Created v1 ["staging", "latest"]: Summarize: {{text}}
Created v2 ["staging", "latest"]: Summarize in three bullet points: {{text}}

Promoted v1 ["production"]: Summarize: {{text}}
label=production -> v1 ["production"]: Summarize: {{text}}
version=2 -> v2 ["staging", "latest"]: Summarize in three bullet points: {{text}}

Prompts in rust-sdk-examples/:
  rust-sdk-examples/label-demo versions [1, 2] labels ["latest", "production", "staging"] (updated 2026-11-11 12:51)

Deleted version 2
Deleted prompt rust-sdk-examples/label-demo
```

### `skills`

Skill management: publishes a version of a skill made of a `SKILL.md` (YAML
frontmatter with `name` and `description`) and a reference file, fetches it
back with file contents, and lists the project's skills. `YamlTool`, the SDK's
YAML helper, builds and parses the frontmatter and demonstrates a deep merge.

```sh
cargo run --example skills
```

- **Source:** [`examples/skills.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/skills.rs)
- **Data:** adds one version to the skill `rust-sdk-demo-skill` on every run,
  kept. Delete versions with `client.skills.delete_version(name, version)`.
- **Calls:** `skills.create_version` → `POST /api/public/skills`,
  `skills.get` → `GET /api/public/skills/{skillName}`, `skills.list`;
  `YamlTool::to_string`, `validate`, `extract_frontmatter`, `get_field`,
  `keys`, `parse_value`, `merge`.
- **Good to know:** the skill's name comes from the frontmatter of
  `SKILL.md`. A version holds the complete file set. New versions get the
  `latest` label; a fetch without label or version returns `production`.
- **Docs:** [Skill management](https://langfuse.com/docs/skill-management/overview),
  [Version control](https://langfuse.com/docs/skill-management/version-control)

```text
Created: rust-sdk-demo-skill v3 (labels: ["latest"])
Files: 2
  - references/checklist.md (286 bytes)
  - SKILL.md (302 bytes)
...
Extracted frontmatter:
  name: rust-sdk-demo-skill
  description: A demo code review skill created from the Rust SDK.
```

---

## Project configuration

### `manage_models`

Registers a custom model definition with prices ($3 and $12 per million
tokens), lists the model definitions and deletes the new one. Langfuse matches
the model name of each generation against the definitions' `match_pattern` to
compute its cost.

```sh
cargo run --example manage_models
```

- **Source:** [`examples/manage_models.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/manage_models.rs)
- **Data:** writes the model `my-fine-tuned-llm` and deletes it at the end.
- **Calls:** `models.create` → `POST /api/public/models`, `models.list`,
  `models.delete` → `DELETE /api/public/models/{id}`.
- **Good to know:** prices are USD per unit (per token here). Custom
  definitions take priority over the ones Langfuse maintains and apply only to
  generations ingested afterwards.
- **Docs:** [Token and cost tracking](https://langfuse.com/docs/observability/features/token-and-cost-tracking)

```text
Registered model: my-fine-tuned-llm (<model id>)
  Match pattern: (?i)^my-fine-tuned-llm.*$
  Unit: Some(Tokens)
  Input price: Some(3e-6)
  Output price: Some(1.2e-5)

All models: 20 of 205 total
...
Deleted model: <model id>
```

### `llm_connections`

Lists the project's LLM connections (the provider keys that the Playground and
LLM-as-a-judge evaluators use) and, when the key allows it, the
organization-level ones. Secrets are never returned.

```sh
cargo run --example llm_connections
EXAMPLE_LLM_API_KEY=sk-dummy cargo run --example llm_connections   # also upsert + delete
```

- **Source:** [`examples/llm_connections.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/llm_connections.rs)
- **Data:** read-only by default. With `EXAMPLE_LLM_API_KEY` it upserts a
  connection named `rust-sdk-example` and deletes it again; the key is not
  checked with the provider, so a dummy value works.
- **Optional variables:** `EXAMPLE_LLM_API_KEY`, `EXAMPLE_LLM_BASE_URL`.
- **Calls:** `llm_connections.list` → `GET /api/public/llm-connections`,
  `list_organization` → `GET /api/public/organizations/llm-connections`
  (organization-scoped key; skipped with a note otherwise), `upsert` →
  `PUT /api/public/llm-connections`, `delete`.
- **Docs:** [LLM connections](https://langfuse.com/docs/administration/llm-connection)

```text
Project LLM connections: 0

Organization connections: skipped (needs an organization-scoped key)

Upserted connection:
  - rust-sdk-example (adapter openai, base URL default, default models: true)
      custom models: my-fine-tuned-model
Deleted: LLM connection successfully deleted
```

### `dashboards`

Builds a custom dashboard with the unstable Dashboards API: three widgets
(observations over time, observations by name, total cost) placed on the
12-column grid in three ways (explicit position, server-chosen position,
resized after placing). Prints a link, pauses so you can look at it, then
deletes everything, also when a step fails.

```sh
cargo run --example dashboards --features unstable
```

- **Source:** [`examples/dashboards.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/dashboards.rs)
- **Data:** writes three widgets and a dashboard, deleted at the end.
- **Needs:** the `unstable` feature; the endpoints under
  `/api/public/unstable` may change.
- **Optional variables:** `DASHBOARD_PAUSE_SECS=<n>` pauses for `n` seconds
  instead of waiting for Enter (without a terminal and without the variable
  there is no pause); `DASHBOARD_ENVIRONMENT=<name>` adds a dashboard-level
  environment filter.
- **Calls:** `unstable_dashboard_widgets.create` / `delete`,
  `unstable_dashboards.create`, `add_placement`, `update_placement`, `get`,
  `delete`; `projects.list`. 14 requests per run.
- **Docs:** [Custom dashboards](https://langfuse.com/docs/metrics/features/custom-dashboards)

```text
Created widget 'Rust SDK example: observations over time' (LineTimeSeries)
Created widget 'Rust SDK example: observations by name' (HorizontalBar)
Created widget 'Rust SDK example: total cost' (Number)
Created dashboard 'Rust SDK example dashboard'
Placed the time series at x=0 y=0 (12x5)
Server placed the next tile at y=5

Layout of 'Rust SDK example dashboard':
  x=0  y=0  12x5  Rust SDK example: observations over time
  x=0  y=5   6x6  Rust SDK example: observations by name
  x=6  y=5   6x6  Rust SDK example: total cost

Open it in Langfuse: https://cloud.langfuse.com/project/<project id>/dashboards/<dashboard id>
...
Deleted dashboard <dashboard id>
Deleted widget 'Rust SDK example: observations over time'
```

---

## Organization administration

Both examples need an **organization-scoped API key** in
`LANGFUSE_PUBLIC_KEY` / `LANGFUSE_SECRET_KEY`. Organization owners create one
under Organization settings → API keys; on Langfuse Cloud these keys belong
to the Enterprise plan. With a project key both examples print what is
missing and exit successfully.

### `organization_admin`

An overview of an organization: its projects with their members and roles,
the organization members, the organization's API keys, and the blob storage
exports with the state of their last sync.

```sh
cargo run --example organization_admin
```

- **Source:** [`examples/organization_admin.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/organization_admin.rs)
- **Data:** read-only.
- **Calls:** `organizations.get_projects` →
  `GET /api/public/organizations/projects`,
  `organizations.get_project_memberships`, `organizations.get_memberships`,
  `organizations.get_api_keys`, `blob_storage_integrations.list` →
  `GET /api/public/integrations/blob-storage`, `get_status`.
- **Docs:** [SCIM and Org API](https://langfuse.com/docs/administration/scim-and-org-api),
  [Export to blob storage](https://langfuse.com/docs/api-and-data-platform/features/export-to-blob-storage)

With a project key:

```text
This example needs an organization-scoped API key; the current key was rejected (Access denied (403): Access denied. The "projects:read" permission is required to access this resource.).
```

### `scim_users`

Inspects the SCIM 2.0 endpoints that identity providers use to provision
users: the service provider configuration, the resource types, and all users
of the organization (SCIM pagination with `startIndex` and `count`).

```sh
cargo run --example scim_users
```

- **Source:** [`examples/scim_users.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/scim_users.rs)
- **Data:** read-only.
- **Optional variables:** `SCIM_USER_EMAIL` looks up one user with the SCIM
  filter `userName eq "<email>"` instead of listing everyone.
- **Calls:** `scim.get_service_provider_config`, `scim.get_resource_types`,
  `scim.list_users` (under `/api/public/scim`).
- **Docs:** [SCIM and Org API](https://langfuse.com/docs/administration/scim-and-org-api)

---

## AI Gateway

The [Langfuse AI Gateway](https://langfuse.com/docs/gateway/overview) (beta)
forwards OpenAI and Anthropic API calls with your organization's provider
connections and records every call as a generation. Both examples need
`LANGFUSE_GATEWAY_KEY` and `LANGFUSE_GATEWAY_HOST` in addition to the project
keys. A gateway key only works on the gateway host of its own region and
cannot call the public API. Model IDs have the form `<connection>:<model>`.

The gateway writes to the *ingestion project* chosen by its admins. If that
is not the project of your API keys, the SDK's observations and the gateway's
generations share a trace ID but live in two projects; the `gateway` example
tells you when that is the case.

If the model lists are empty, or a call fails with
`404 No eligible provider connection`, no provider connection is enabled for
the key ([troubleshooting](https://langfuse.com/faq/all/ai-gateway-error-codes)).

### `gateway`

Calls models through the gateway from inside an SDK trace: lists the models of
the key, sends an Anthropic Messages request with Claude Sonnet 5.5 and a
streamed one with Claude Opus 5.5 from an agent observation, and prints the
trace and observation ID of each generation the gateway recorded. The calls
carry the observation's `traceparent`, so the gateway's generations become
children of the agent observation.

```sh
cargo run --example gateway --features otel
```

- **Source:** [`examples/gateway.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/gateway.rs)
- **Data:** writes one SDK trace and two gateway generations, kept. The model
  calls are billed by your provider.
- **Needs:** the `otel` feature and the gateway variables.
- **Optional variables:** `GATEWAY_ANTHROPIC_MODEL`,
  `GATEWAY_ANTHROPIC_STREAM_MODEL` (other models), `GATEWAY_OPENAI_MODEL`
  (adds a Chat Completions call; by default only when the OpenAI route offers
  Claude Sonnet or Opus 5.5).
- **Calls:** `GatewayClient::from_env`, `anthropic_models`, `openai_models`,
  `messages`, `messages_stream`, `chat_completions`, `count_tokens`
  (`{host}/anthropic/v1/...` and `{host}/openai/v1/...`).
- **Good to know:** do not start a generation around a gateway call; the
  gateway records it, and tracing it again would duplicate it.
- **Docs:** [Gateway tracing](https://langfuse.com/docs/gateway/features/tracing),
  [REST API](https://langfuse.com/docs/gateway/features/rest-api)

```text
gateway: https://gateway.us.cloud.langfuse.com
26 Anthropic and 65 OpenAI models; using amazon-bedrock:us.anthropic.claude-sonnet-5-5, amazon-bedrock:us.anthropic.claude-opus-5-5 (stream) and no Claude 5.5 on the OpenAI route: chat completions skipped
messages: trace <trace id> generation <observation id> (provider bedrock, request <request id>)
  "A gateway gives you a single control point for all model calls, ..."
messages_stream: trace <trace id> generation <observation id> (provider bedrock, request <request id>)
  1, 2, 3, 4, 5
count_tokens unavailable: {"Output":{"__type":"com.amazon.coral.service#UnknownOperationException"},"Version":"1.0"}

SDK trace: https://us.cloud.langfuse.com/project/<project id>/traces/<trace id>
```

### `gateway_openai`

Uses the plain `async-openai` client with the gateway:
`GatewayClient::openai_config()` points it at the gateway's OpenAI route with
the gateway key and adds the trace context of the current observation to
every request. Sends one Responses API request inside a span.

```sh
cargo run --example gateway_openai --features openai
GATEWAY_OPENAI_MODEL=<connection>:<model> cargo run --example gateway_openai --features openai
```

- **Source:** [`examples/gateway_openai.rs`](https://github.com/lep511/langfuse-rust-sdk/blob/main/examples/gateway_openai.rs)
- **Data:** when a model is called, writes one SDK trace and one gateway
  generation, kept; the call is billed by your provider.
- **Needs:** the `openai` feature and the gateway variables.
- **Optional variables:** `GATEWAY_OPENAI_MODEL`. Without it the example uses
  Claude Sonnet 5.5 or Opus 5.5 if the OpenAI route offers them, and otherwise
  prints a note and stops without calling a model: the gateway does not
  translate between API formats, and the example never picks a model that
  Langfuse has no price for.
- **Calls:** `GatewayClient::with_headers`, `openai_config`,
  `async_openai::Client::with_config`, `responses().create`.
- **Docs:** [OpenAI SDK with the gateway](https://langfuse.com/integrations/model-providers/openai-gateway)

```text
<connection>:<model>: An AI gateway centralizes and controls access to AI models and services, ...
SDK trace: https://us.cloud.langfuse.com/project/<project id>/traces/<trace id>
```

---

## Verification

All 31 examples were built with `--all-features` and run on 2026-11-11
against a test project on Langfuse Cloud (US region, server 4.78.0), with the
library of SDK 0.11.2, which 0.11.3 leaves unchanged. The sample outputs above
come from those runs. The claims in the doc
comments and in this file were checked against the official documentation
linked in each section and against `api/openapi.yml`.

| Result | Examples |
|---|---|
| Passed end to end | the 27 examples not listed below |
| Passed on the project-key path only | `organization_admin`, `scim_users`: the test key is project-scoped, so they printed the note about the missing organization key and exited successfully; the listing code did not run |
| Passed against a local OpenAI stub | `openai_tracing`: no `OPENAI_API_KEY` was available, so `OPENAI_BASE_URL` pointed at a local OpenAI-compatible stub. The resulting trace (agent, two generations, embedding) was read back from Langfuse |
| Passed with a model named explicitly | `gateway_openai`: the test gateway's OpenAI route has no Claude 5.5, so the default run stops before the model call; with `GATEWAY_OPENAI_MODEL` set it completed |

Of the optional variables, `EXAMPLE_LLM_API_KEY`, `DASHBOARD_PAUSE_SECS` and
`GATEWAY_OPENAI_MODEL` were used. The branches behind
`LANGFUSE_EXAMPLE_USER_ID`, `LANGFUSE_TRACE_ID`, `DASHBOARD_ENVIRONMENT`,
`KEEP_TRACE`, `KEEP_EXPERIMENT`, `SCIM_USER_EMAIL` and the organization part
of `llm_connections` were not run.
