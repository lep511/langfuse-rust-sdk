# langfuse-rust-sdk

Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.

## Features

- Covers all 120 operations of the Langfuse public API (`openapi.yml`); the 21 unstable ones are behind the `unstable` feature
- Async-only API built on `reqwest` + `tokio`
- Typed models with `serde`, including tagged unions (`ScoreV3` on `dataType`, `Prompt` on `type`, `Evaluator` on `type`, ...)
- Response enums tolerate new server values (`Unknown` variant)
- Automatic retry with exponential backoff, jitter, and `Retry-After` support
- HTTP Basic Auth with SDK identification headers
- Cursor-based and page-based pagination, with lazy `*_pages` streams that follow every page
- Configuration from `LANGFUSE_*` environment variables (`ClientConfig::from_env`)
- Trace and observation IDs, random or derived from a seed (`ids::create_trace_id(Some(seed))`)
- Prompt management: client-side cache with background refresh and fallbacks (`prompts.get_cached`), `{{variable}}` and placeholder compilation
- `auth_check()` and `trace_url(trace_id)` helpers; `LangfuseClient` is cheap to `Clone`
- Tracing on OpenTelemetry (feature `otel`): observations, context and attribute propagation, sampling, masking and batched export
- Scores queued and sent in the background in batches (`client.score`)
- Media attachments: upload, reference strings, resolution back to data URIs, and automatic upload of base64 media in traces and dataset items
- Datasets with all their items (`client.get_dataset`), experiments on datasets or local data with item, composite and run evaluators (`tracer.run_experiment`, feature `otel`), and batch evaluation of stored observations (`client.run_batched_evaluation`)

## Supported Resources

| Client | Endpoints |
|---|---|
| `health` | Health check |
| `scores` | Create (batch), list (cursor), delete |
| `score_configs` | Create, list (page), get, update |
| `observations` | List (cursor) |
| `datasets` | Create, list (page), get by name |
| `dataset_items` | Create, list (page), get, delete |
| `prompts` | Create, list (page), get, get through the cache, delete (all versions or by label/version) |
| `prompt_versions` | Update labels |
| `models` | Create, list (page), get, upsert, delete |
| `skills` | Create version, list (page), get, update tags, set labels, delete version, get file contents |
| `projects` | Create, list, update, delete, list/create/delete API keys |
| `traces` | Delete single, delete batch |
| `ingestion` | Ingest OTel traces |
| `media` | Get, patch upload status, get upload URL |
| `comments` | Create, list (page), get |
| `feedback` | Submit |
| `annotation_queues` | Queues (create, list, get), items (create, list, get, update, delete), assignments (create, delete) |
| `blob_storage_integrations` | List, upsert, get status, delete |
| `evaluation_rules` | Create, list (cursor), get, update, delete |
| `evaluators` | Create, list (cursor), get, update, delete, list versions |
| `experiments` | List experiments, list experiment items (cursor) |
| `llm_connections` | List, upsert, delete (project and organization level) |
| `metrics` | Query (typed `MetricsQuery`) |
| `organizations` | Memberships (organization and project), projects, API keys |
| `scim` | Service provider config, resource types, schemas, users (list, create, get, delete) |
| `unstable_dashboard_widgets` | List, create, get, update, delete (feature `unstable`) |
| `unstable_dashboards` | Dashboards and placements (feature `unstable`) |
| `unstable_topics` | Facets, topics, rules, summaries, assignments (feature `unstable`) |

Organization-level endpoints (`organizations`, `scim`, `blob_storage_integrations`, project API keys, organization LLM connections) need an organization-scoped API key.

The unstable APIs may change without notice upstream. Enable them with:

```toml
[dependencies]
langfuse-rust-sdk = { version = "0.7", features = ["unstable"] }
```

## Quick Start

```rust
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue, CreateScoresResponse};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::builder("pk-lf-...", "sk-lf-...")
        .base_url("https://cloud.langfuse.com")
        .build()?;

    let client = LangfuseClient::new(config)?;

    // Health check
    let health = client.health.check().await?;
    println!("Langfuse {} — {}", health.version, health.status);

    // List datasets
    let datasets = client
        .datasets
        .list(&PageParams { page: Some(1), limit: Some(10) })
        .await?;
    for ds in &datasets.data {
        println!("  {} ({})", ds.name, ds.id);
    }

    // Create scores (the endpoint takes a batch)
    let score = CreateScoreRequest {
        trace_id: Some("trace-abc".into()),
        ..CreateScoreRequest::new("accuracy", CreateScoreValue::Numeric(0.95))
    };
    match client.scores.create(&[score]).await? {
        CreateScoresResponse::Accepted(resp) => println!("Scores {}", resp.message),
        CreateScoresResponse::BatchResults(r) => {
            println!("{} accepted, {} rejected", r.accepted, r.rejected)
        }
    }

    Ok(())
}
```

## Examples

All examples are in the `examples/` directory. They auto-load a `.env` file via `dotenvy`, so you can either export environment variables or create a `.env` file:

```sh
# Option 1: .env file (recommended)
cat > .env <<EOF
LANGFUSE_PUBLIC_KEY=pk-lf-...
LANGFUSE_SECRET_KEY=sk-lf-...
LANGFUSE_BASE_URL=https://cloud.langfuse.com
EOF

cargo run --example <name>
cargo run --example dashboards --features unstable   # unstable APIs

# Option 2: inline env vars
LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example <name>
```

| Example | Description |
|---|---|
| [`health_check`](examples/health_check.rs) | Verify the server is healthy and the API keys are valid (read-only) |
| [`create_score`](examples/create_score.rs) | Create numeric and categorical scores for a trace in one batch |
| [`list_scores`](examples/list_scores.rs) | Paginate through all scores using cursor-based pagination |
| [`score_configs`](examples/score_configs.rs) | Create numeric and categorical score configurations |
| [`manage_datasets`](examples/manage_datasets.rs) | Create a dataset, upsert items, fetch it with all its items, and list all datasets |
| [`prompts`](examples/prompts.rs) | Create text and chat prompts (with a placeholder), fetch by label, and list all |
| [`manage_models`](examples/manage_models.rs) | Register custom models with pricing, list, and delete |
| [`skills`](examples/skills.rs) | Create a skill version with files, fetch with content, list all |
| [`observations`](examples/observations.rs) | List recent observations with a formatted table output |
| [`otel_ingestion`](examples/otel_ingestion.rs) | Ingest an OpenTelemetry span with tags and print its URL in the UI (writes one trace) |
| [`otel_tracing`](examples/otel_tracing.rs) | Trace a request with nested observations, user/session attributes and a score (`--features otel`, writes one trace) |
| [`media`](examples/media.rs) | Attach an image to a generation and resolve it back to a data URI (`--features otel`, writes a trace and media) |
| [`error_handling`](examples/error_handling.rs) | Handle config errors, 404s, auth failures, and graceful fallbacks |
| [`metrics_query`](examples/metrics_query.rs) | Query cost and tokens per model and a daily trend with the Metrics API (read-only) |
| [`experiments`](examples/experiments.rs) | List recent experiments with scores and inspect their items (read-only) |
| [`run_experiment`](examples/run_experiment.rs) | Run an experiment on local data with evaluators, check it is listed, then delete it (`--features otel`, writes traces and scores) |
| [`batch_evaluation`](examples/batch_evaluation.rs) | Score the generations of a trace with `run_batched_evaluation`, then delete it (writes a trace and scores) |
| [`comments`](examples/comments.rs) | Comment on a trace, then list and fetch its comments (writes a comment) |
| [`annotation_queues`](examples/annotation_queues.rs) | Queue a trace for human review, complete it, and remove the item (writes data) |
| [`evaluators`](examples/evaluators.rs) | Create an LLM-as-a-judge evaluator and a disabled evaluation rule, then delete both |
| [`prompt_labels`](examples/prompt_labels.rs) | Create prompt versions in a folder, promote one to `production`, filter, and delete |
| [`llm_connections`](examples/llm_connections.rs) | List LLM connections; optionally upsert and delete a temporary one |
| [`organization_admin`](examples/organization_admin.rs) | Projects, members, API keys and blob storage exports (organization key, read-only) |
| [`scim_users`](examples/scim_users.rs) | SCIM provider config and paginated user list (organization key, read-only) |
| [`dashboards`](examples/dashboards.rs) | Build a dashboard with three widgets, pause so you can open it in the UI, then delete it (`--features unstable`) |

## Configuration

`ClientConfig::from_env()` reads these variables:

| Variable | Use |
|---|---|
| `LANGFUSE_PUBLIC_KEY` | Public API key (required) |
| `LANGFUSE_SECRET_KEY` | Secret API key (required) |
| `LANGFUSE_BASE_URL` | API base URL (default: `https://cloud.langfuse.com`); `LANGFUSE_HOST` is read as a deprecated fallback |
| `LANGFUSE_TIMEOUT` | Request timeout in seconds (default: 30) |

```rust
use langfuse_rust_sdk::{ClientConfig, ConfigBuilder, LangfuseClient};

let client = LangfuseClient::new(ClientConfig::from_env()?)?;
// Or start from the environment and override some settings:
let config = ConfigBuilder::from_env()?.header("x-team", "search").build()?;
```

Every setting can also be given explicitly:

```rust
use langfuse_rust_sdk::{ClientConfig, RetryPolicy};
use std::time::Duration;

let config = ClientConfig::builder("pk-lf-...", "sk-lf-...")
    .base_url("https://self-hosted.example.com")  // default: https://cloud.langfuse.com
    .timeout(Duration::from_secs(60))              // default: 30s
    .retry_policy(RetryPolicy {
        initial_backoff: Duration::from_millis(500),
        max_backoff: Duration::from_secs(30),
        max_retries: 3,                            // default: 2
        jitter_fraction: 0.2,
    })
    .header("x-custom-header", "value")            // invalid headers make build() fail
    .http_client(reqwest::Client::new())           // optional: proxy, custom TLS, shared pool
    .build()
    .unwrap();
```

A base URL with a path (`https://example.com/langfuse`) is kept as a prefix of every request.
`LangfuseClient` is cheap to clone: clones share the connection pool, so move one into each task.

## Prompts

`prompts.get_cached` serves prompts from an in-memory cache (shared by clones of the client):
a fresh prompt is returned without a request, an expired one is returned immediately while a
background task refreshes it, and a fallback can cover the first fetch when Langfuse is
unreachable. Creating, deleting or relabelling a prompt through the client invalidates it.

```rust
use langfuse_rust_sdk::api::prompts::{GetPromptOptions, PromptFallback};
use langfuse_rust_sdk::models::prompts::ChatMessage;
use serde_json::json;
use std::time::Duration;

let options = GetPromptOptions::default()
    .with_label("production")                             // default; or .with_version(3)
    .with_cache_ttl(Duration::from_secs(300))             // default: 60 s (`prompt_cache_ttl`)
    .with_fallback(PromptFallback::Text("Summarize: {{text}}".into()));
    // also: with_max_retries, with_fetch_timeout
let prompt = client.prompts.get_cached("summarizer", &options).await?;
println!("{:?}", prompt.variables());                     // ["text"]
let text = prompt.compile(&json!({"text": "..."}))?.into_text();

// Chat prompts: fill placeholders with message lists; their contents are compiled too.
let history = vec![ChatMessage { role: "user".into(), content: "Hi, I'm {{name}}".into() }];
let messages = client
    .prompts
    .get_cached("support-chat", &GetPromptOptions::default())
    .await?
    .compile_with_placeholders(&json!({"name": "Ada"}), [("history", &history)])?
    .into_chat();
```

When placeholder messages carry more than `role` and `content` (tool calls, tool results),
use `compile_with_json_placeholders`: it accepts any serializable messages, such as the request
types of an LLM client crate, keeps every field and returns JSON messages to convert back with
`serde_json::from_value`.

Variables come from anything that serializes to a JSON object (`json!`, maps, structs).
Unknown `{{variables}}` are left untouched, `null` becomes an empty string and non-string values
are inserted as compact JSON.

## Tracing (feature `otel`)

```toml
[dependencies]
langfuse-rust-sdk = { version = "0.7", features = ["otel"] }
```

A `LangfuseTracer` creates observations as OpenTelemetry spans and exports them in the
background. Make an observation current with `in_scope` so the ones started inside become its
children, and add trace attributes to a whole scope with `propagate_attributes`:

```rust
use langfuse_rust_sdk::otel::{ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes};
use serde_json::json;

let tracer = client.tracer(TracingOptions::from_env()?)?;   // inside a Tokio runtime

let request = tracer.start_observation("chat-request", ObservationType::Agent);
request.set_input(&json!({"question": "Hi"}));

let user = PropagatedAttributes::default()
    .with_user_id("user-42")
    .with_session_id("session-7")
    .with_tag("beta");
request
    .in_scope(propagate_attributes(user, async {
        let generation = tracer.start_observation("llm", ObservationType::Generation);
        generation.set_model("gpt-4o").set_input("Hi");
        // ... call the model ...
        generation.set_output("Hello!").set_usage_details(&json!({"input": 3, "output": 2}));
        generation.end();
    }))
    .await;
request.set_output("Hello!");
request.end();

tracer.shutdown().await;   // export queued spans before exiting
```

- **Types:** span, generation, event, agent, tool, chain, retriever, embedding, evaluator,
  guardrail. Model, usage, cost, completion start time and prompt apply to generations and
  embeddings.
- **Context:** `Observation::current()`, `otel::current_trace_id()`,
  `otel::current_observation_id()`. The context follows the future, not spawned tasks: wrap
  those with `in_scope` too. `tracer.start_observation_in_trace(.., &TraceContext)`
  continues a trace started elsewhere.
- **Options** (`TracingOptions::default().with_*(..)`, or `from_env()` with `LANGFUSE_TRACING_ENVIRONMENT`,
  `LANGFUSE_RELEASE`, `LANGFUSE_SAMPLE_RATE`, `LANGFUSE_TRACING_ENABLED`, `LANGFUSE_FLUSH_AT`,
  `LANGFUSE_FLUSH_INTERVAL`, `LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES`): environment, release,
  sampling, enabled, batching, `mask` (redacts input, output and metadata) and
  `should_export_span`.
- **Other instrumentation:** `tracer.provider()` is a regular `SdkTracerProvider`. Spans of
  other libraries are exported when they have `gen_ai.*` attributes or come from a known LLM
  instrumentation scope (`is_default_export_span`), or whatever `should_export_span` accepts.
  `#[tracing::instrument]` spans can be bridged with `tracing-opentelemetry` (see the `otel`
  module docs).
- **Scores:** `observation.score(..)`, `observation.score_trace(..)`,
  `tracer.score_current_span(..)` and `tracer.score_current_trace(..)` queue scores on the
  client. `tracer.score(..)` queues any score but drops the ones whose trace is sampled out
  (`client.score` does not know about sampling).
- **Media:** base64 data URIs (and the base64 blocks of common LLM APIs) in exported span
  attributes are uploaded and replaced by reference strings; `with_media_upload(false)` turns it
  off.
- **Flushing:** `tracer.flush()` / `tracer.shutdown()` export the spans first, then flush the
  client's score and media queues (the export queues the media uploads), so one call covers
  everything the tracer produced.

## Scores in the background

`client.score(..)` queues a score and returns at once; a background task sends scores in
batches (15 scores or every second by default, see `ConfigBuilder::score_queue` and
`LANGFUSE_FLUSH_AT` / `LANGFUSE_FLUSH_INTERVAL`). Call `flush` before a short-lived program exits.
Use `scores.create` instead to send immediately and inspect the response.

```rust
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};

client.score(CreateScoreRequest {
    trace_id: Some(trace_id.clone()),
    comment: Some("Thumbs up from the user".into()),
    ..CreateScoreRequest::new("user-feedback", CreateScoreValue::Numeric(1.0))
});
client.flush().await;
```

## Media

```rust
use langfuse_rust_sdk::media::{LangfuseMedia, MediaField, MediaTarget};
use langfuse_rust_sdk::models::media::MediaContentType;

let image = LangfuseMedia::from_file("chart.png", MediaContentType::ImagePng)?;
image.upload(&client, &MediaTarget::observation(&trace_id, &observation_id, MediaField::Input)).await?;
let reference = image.reference_string();   // "@@@langfuseMedia:type=image/png|id=...|source=file@@@"

// Data fetched from Langfuse contains reference strings; turn them back into data URIs:
let resolved = client.media.resolve_references(&observation_input).await;
```

With the `otel` feature you rarely upload by hand: put the media (or any base64 data URI) in the
traced data, e.g. `span.set_input(&json!({"image": image}))`, and it is uploaded during export.

## Datasets and experiments

`client.get_dataset(name, &GetDatasetOptions::default())` returns the dataset with every item
(`.with_version(time)` reads them as they were then). `client.create_dataset_item(&request)`
uploads the media found in the item first.

With the `otel` feature, `tracer.run_experiment(data, task, options)` runs a task on dataset items
or local data, traces each item, runs the evaluators and returns the results; Langfuse builds the
experiment from the traces. `dataset.run_experiment(&tracer, task, options)` does the same on a
fetched dataset, and runs with the same run name share one experiment.

```rust
use langfuse_rust_sdk::experiment::{
    Evaluation, EvaluatorInput, ExperimentItem, ExperimentOptions, LocalExperimentItem,
    RegressionError, evaluator,
};

let data = vec![LocalExperimentItem::new("Capital of France?").with_expected_output("Paris")];
let exact_match = evaluator("exact_match", |args: EvaluatorInput| async move {
    Evaluation::boolean("exact_match", args.expected_output == Some(args.output))
});
let result = tracer
    .run_experiment(
        data,
        |item: ExperimentItem| async move { ask_llm(item.input().unwrap()).await },
        ExperimentOptions::new("capitals").with_evaluator(exact_match),
    )
    .await?;
println!("{result}");   // summary; `{result:#}` adds every item
if result.average("exact_match").unwrap_or(0.0) < 0.9 {
    return Err(RegressionError::new(result).with_message("accuracy dropped").into());
}
```

Item evaluations become scores on the item's task span, run evaluations
(`with_run_evaluator`) scores on the experiment; `with_composite_evaluator` combines the
evaluations of each item. Failing tasks and evaluators are recorded and skipped.

### Batch evaluation

`client.run_batched_evaluation(options)` scores observations already in Langfuse with the same
evaluators: it pages through `GET /api/public/v2/observations` (with a JSON `filter`), maps each
observation to `EvaluatorInput` and queues one score per evaluation. A run stopped by `max_items`
or a failed request returns a `BatchEvaluationResumeToken` to continue it:

```rust
use langfuse_rust_sdk::batch_evaluation::BatchEvaluationOptions;

let mapper = |o: ObservationV2| async move {
    EvaluatorInput::new(o.input.unwrap_or_default(), o.output.unwrap_or_default())
};
let options = BatchEvaluationOptions::new(mapper)
    .with_evaluator(not_empty)
    .with_filter(r#"[{"type": "string", "column": "name", "operator": "=", "value": "chat"}]"#)
    .with_max_items(500);
let result = client.run_batched_evaluation(options).await?;
println!("{result}");
let next = result.resume_token;   // serializable; pass it to `with_resume_from`
```

## Pagination

Every list method returning `Paginated<T>` or `CursorPaginated<T>` has a `*_pages` companion
(`observations.list_pages`, `datasets.list_pages`, `annotation_queues.list_items_pages`, ...)
that returns a lazy stream of pages. `.items()` flattens it into a stream of items, and
`.take(n)` caps the number of items without requesting further pages:

```rust
use futures::TryStreamExt;
use langfuse_rust_sdk::api::observations::ListObservationsParams;

let params = ListObservationsParams {
    trace_id: Some("abc123".into()),
    ..Default::default()
};
let mut observations = client.observations.list_pages(params).items();
while let Some(observation) = observations.try_next().await? {
    println!("{} {:?}", observation.id, observation.name);
}
```

## IDs, auth check and trace URLs

```rust
use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};

let trace_id = create_trace_id(Some("external-request-42")); // same seed, same ID
let span_id = create_observation_id(None);                    // random, 16 hex chars

let project = client.auth_check().await?;                     // verifies the API keys
let url = client.trace_url(&trace_id).await?;                 // {base_url}/project/{id}/traces/{trace_id}
```

## Error Handling

All API methods return `Result<T, LangfuseError>`. The error type maps HTTP status codes to specific variants:

| Status | Variant |
|---|---|
| 400 | `BadRequest` |
| 401 | `Unauthorized` |
| 403 | `AccessDenied` |
| 404 | `NotFound` |
| 405 | `MethodNotAllowed` |
| 409 | `Conflict` |
| 412 | `PreconditionFailed` |
| 429 | `RateLimited` (with optional `retry_after` duration) |
| 503 | `ServiceUnavailable` |
| other 5xx | `ServerError` |

`LangfuseError` is `#[non_exhaustive]`, so `match` statements need a wildcard arm. `err.body()` returns the parsed JSON error body. The evaluators, evaluation-rules and unstable endpoints return a structured body that `PublicApiError::from_error(&err)` (or `UnstablePublicApiError::from_error`) parses.

Failed requests are retried with exponential backoff, honoring `Retry-After`, as long as repeating them cannot duplicate their effect:

| Failure | GET, HEAD, PUT, DELETE | POST, PATCH |
|---|---|---|
| 408, 429, 503 (request not processed) | retried | retried |
| Other 5xx | retried | not retried |
| Connection failure (request never sent) | retried | retried |
| Timeout or connection dropped mid-request | retried | not retried |

OTLP trace ingestion is retried like an idempotent request, since spans carry their IDs. 409 is never retried, because it signals a real conflict. See [`error_handling`](examples/error_handling.rs) for patterns.

## Project Structure

```
src/
  lib.rs            — LangfuseClient, crate root, re-exports
  config.rs         — ClientConfig builder
  error.rs          — LangfuseError enum
  http.rs           — HttpClient (reqwest wrapper with auth, retry, path encoding)
  retry.rs          — RetryPolicy (exponential backoff + jitter)
  pagination.rs     — Paginated<T>, CursorPaginated<T>, query param types, Pages/Items streams
  ids.rs            — Trace and observation ID generation (random or seeded)
  prompt_cache.rs   — In-memory cache behind prompts.get_cached (crate-private)
  otel/             — Tracing on OpenTelemetry (feature `otel`): tracer, observations,
                      propagation, span processor, batched exporter, run_experiment
  media.rs          — LangfuseMedia, references, upload, media detection in traced data
  dataset.rs        — get_dataset and create_dataset_item (crate-private module)
  experiment.rs     — Evaluation, evaluator and task traits, experiment options and results
  batch_evaluation.rs — run_batched_evaluation, resume tokens and results
  background.rs     — Score queue and media upload tasks of LangfuseClient (crate-private)
  models/           — Typed request/response structs for all resources
  api/              — Resource clients (one per API group)
examples/           — 25 runnable examples covering all major features
tests/              — wiremock HTTP tests (api_*), serde tests, config tests
                      (572 tests in total with --all-features, 480 without)
```

## Requirements

- Rust 1.99+ (edition 2024)
- tokio runtime

## License

MIT
