# langfuse-rust-sdk

Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.

## Features

- Covers all 120 operations of the Langfuse public API (`openapi.yml`); the 21 unstable ones are behind the `unstable` feature
- Async API built on `reqwest` + `tokio`, plus a blocking client for programs without a runtime (feature `blocking`)
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
- `#[observe]` to trace plain functions, sync or async, including streams and iterators (feature `macros`)
- Integrations: traced OpenAI calls through `async-openai` (feature `openai`), traced Amazon Bedrock Converse calls through `aws-sdk-bedrockruntime` (feature `bedrock`), and a `tracing` bridge that records rig agents and `#[tracing::instrument]` spans (feature `tracing-layer`)
- Token usage of other providers (Anthropic, Bedrock, Vertex AI / Gemini, IBM, LangChain-style) translated into Langfuse usage details (`usage::normalize_usage`, `Observation::set_provider_usage`)
- Skills discovery (`skills::discover_skills`) and the skills available to an agent on its spans
- Per-request options on a client view (`client.with_options`): timeout, retries, headers, query and body parameters, response headers
- [Langfuse AI Gateway](https://langfuse.com/docs/gateway/overview) (beta): `gateway::GatewayClient` calls the gateway's OpenAI and Anthropic routes (JSON and streams), with the `langfuse-*` request headers and, with `otel`, the trace context of the current observation, so the gateway's generations join the SDK's traces; with `openai`, an `async-openai` configuration for the gateway

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
langfuse-rust-sdk = { version = "0.10", features = ["unstable"] }
```

### Cargo features

| Feature | Adds |
|---|---|
| `unstable` | The 21 `/api/public/unstable/*` operations |
| `otel` | Tracing on OpenTelemetry (`otel` module), `run_experiment` |
| `macros` | `#[observe]` (enables `otel`) |
| `tracing-layer` | `otel::tracing_layer`, the bridge from the `tracing` crate (enables `otel`) |
| `openai` | `openai::OpenAI`, traced `async-openai` 0.43 calls, and `gateway::GatewayOpenAIConfig` (enables `otel`) |
| `bedrock` | `bedrock::Bedrock`, traced `aws-sdk-bedrockruntime` Converse calls (enables `otel`) |
| `blocking` | `blocking::LangfuseClient`, on an internal Tokio runtime |

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

The examples are in the `examples/` directory, except `rig_tracing` and `bedrock_tracing`, which live in the unpublished `integration-tests/` workspace member (so that the crate's own builds never compile rig or the AWS SDK; run them with `-p langfuse-rust-sdk-integration-tests`). They auto-load a `.env` file via `dotenvy`, so you can either export environment variables or create a `.env` file. [`examples/EXAMPLES.md`](examples/EXAMPLES.md) describes each example in detail: what it writes and leaves behind, the features and variables it needs, the calls it makes and a sample of its output. The same guide is on docs.rs as the [`examples`](https://docs.rs/langfuse-rust-sdk/latest/langfuse_rust_sdk/examples/index.html) module, where the items of the API reference also show the snippets of the examples that use them.

```sh
# Option 1: .env file (recommended)
cat > .env <<EOF
LANGFUSE_PUBLIC_KEY=pk-lf-...
LANGFUSE_SECRET_KEY=sk-lf-...
LANGFUSE_BASE_URL=https://cloud.langfuse.com
EOF

cargo run --example <name>
cargo run --example dashboards --features unstable   # unstable APIs
cargo run -p langfuse-rust-sdk-integration-tests --example bedrock_tracing --features bedrock

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
| [`observations`](examples/observations.rs) | List the observations of the last 24 hours with a formatted table output |
| [`otel_ingestion`](examples/otel_ingestion.rs) | Ingest an OpenTelemetry span with tags and print its URL in the UI (writes one trace) |
| [`otel_tracing`](examples/otel_tracing.rs) | Trace a request with nested observations, user/session attributes and a score (`--features otel`, writes one trace) |
| [`otel_export_options`](examples/otel_export_options.rs) | Gzip export, a provider shared with another exporter and `mask_otel_spans` redaction, checked through the API (`--features otel`, writes one trace and deletes it) |
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
| [`observe`](examples/observe.rs) | Trace an agent, a retriever, a streamed generation and a failing tool with `#[observe]`, with skills (`--features macros`, writes one trace) |
| [`rig_tracing`](integration-tests/examples/rig_tracing.rs) | Trace a rig agent through `otel::tracing_layer` (`-p langfuse-rust-sdk-integration-tests --features tracing-layer`, needs an OpenAI-compatible endpoint, writes one trace) |
| [`openai_tracing`](examples/openai_tracing.rs) | Trace a chat completion, a streamed one and an embedding made with async-openai (`--features openai`, needs `OPENAI_API_KEY`, writes one trace) |
| [`bedrock_tracing`](integration-tests/examples/bedrock_tracing.rs) | Trace a Converse call and a streamed one made with aws-sdk-bedrockruntime (`-p langfuse-rust-sdk-integration-tests --features bedrock`, needs AWS credentials with Bedrock access, writes one trace) |
| [`gateway`](examples/gateway.rs) | Call Claude Sonnet 5.5 and a streamed Claude Opus 5.5 through the Langfuse AI Gateway inside an SDK span, so the gateway's generations join its trace (`--features otel`, needs `LANGFUSE_GATEWAY_KEY` and `LANGFUSE_GATEWAY_HOST`, writes one trace and the gateway's generations) |
| [`gateway_openai`](examples/gateway_openai.rs) | async-openai through the gateway with `GatewayClient::openai_config` (`--features openai`, needs the gateway variables and Claude Sonnet/Opus 5.5 on the gateway's OpenAI route or `GATEWAY_OPENAI_MODEL`, writes one trace and one gateway generation) |
| [`blocking`](examples/blocking.rs) | The blocking client from a plain `main`, with a per-request options view (`--features blocking`, read-only) |

## Configuration

`ClientConfig::from_env()` reads these variables:

| Variable | Use |
|---|---|
| `LANGFUSE_PUBLIC_KEY` | Public API key (required) |
| `LANGFUSE_SECRET_KEY` | Secret API key (required) |
| `LANGFUSE_BASE_URL` | API base URL (default: `https://cloud.langfuse.com`); `LANGFUSE_HOST` is read as a deprecated fallback |
| `LANGFUSE_TIMEOUT` | Request timeout in seconds (default: 30, with a 5 s connect timeout) |

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
    .timeout(Duration::from_secs(60))              // default: 30s (connect: 5s)
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

### Per-request options

`client.with_options(options)` returns a view of the client that applies `RequestOptions` to
every request (the `request_options` of the Python SDK). It shares the pool, the prompt cache and
the background queues, and views nest:

```rust
use langfuse_rust_sdk::RequestOptions;
use std::time::Duration;

let quick = client.with_options(
    RequestOptions::default()
        .with_timeout(Duration::from_secs(2))
        .with_max_retries(0)                      // the retry rules stay those of the SDK
        .with_header("x-request-source", "batch-job")
        .with_query_param("debug", "1")
        .with_response_hook(|response| println!("{} {:?}", response.status, response.headers)),
);
let health = quick.health.check().await?;
```

`with_body_param` merges fields into JSON object bodies. Scores and media uploads queued in the
background keep the options of the original client.

### Debug logging

The SDK logs through the [`tracing`](https://docs.rs/tracing) crate (the equivalent of the
Python SDK's `LANGFUSE_DEBUG`): requests and retries, exports, dropped spans and scores, failed
uploads. Nothing is printed until the application installs a subscriber; with
`tracing-subscriber` (feature `env-filter`), `RUST_LOG=langfuse_rust_sdk=debug` shows the SDK's
debug output:

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

Warnings and errors (failed exports, full queues, invalid settings read from the environment)
use the `warn` and `error` levels, so `RUST_LOG=langfuse_rust_sdk=warn` keeps only those.

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
    // also: with_max_retries, with_fetch_timeout (default: 5 s per attempt, or the
    // client timeout if you set one)
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
langfuse-rust-sdk = { version = "0.10", features = ["otel"] }
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
  `LANGFUSE_FLUSH_INTERVAL`, `LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES`, `LANGFUSE_OTEL_COMPRESSION`,
  `LANGFUSE_OTEL_TRACES_EXPORT_PATH`): environment, release, sampling, enabled, batching, `mask`
  (redacts input, output and metadata) and `should_export_span`. Without `LANGFUSE_RELEASE`,
  `from_env` takes the release from the commit variable of common CI platforms (`GITHUB_SHA`,
  `CI_COMMIT_SHA`, ...), and `OTEL_SDK_DISABLED=true` disables tracing, as in the Python SDK.
- **Export:** gzip-compressed by default (`with_compression(Compression::None)` sends plain
  JSON; `from_env` also reads `OTEL_EXPORTER_OTLP_TRACES_COMPRESSION` and
  `OTEL_EXPORTER_OTLP_COMPRESSION`). `with_traces_export_path(..)` changes the endpoint path
  under the base URL, `with_mask_otel_spans(|spans| ..)` edits the attributes of every export
  batch before it is sent (a panic drops the batch), `with_id_generator(..)` sets the trace and
  span IDs, and `with_span_exporter(..)` sends the spans to another OpenTelemetry exporter
  instead of Langfuse.
- **Your own provider:** `LangfuseSpanProcessor::new(&client, options)` gives the Langfuse
  processor; add a clone to a `SdkTracerProvider` that also feeds other backends and build the
  tracer with `LangfuseTracer::with_provider(&processor, provider)` (see
  `examples/otel_export_options.rs`).
- **Other instrumentation:** `tracer.provider()` is a regular `SdkTracerProvider`. Spans of
  other libraries are exported when they have `gen_ai.*` attributes or come from a known LLM
  instrumentation scope (`is_default_export_span`), or whatever `should_export_span` accepts.
  `otel::tracing_layer(&tracer)` (feature `tracing-layer`) bridges the `tracing` crate.
- **Existing traces:** `otel::in_trace(TraceContext, future)` attaches the observations started
  in a scope to a trace (and parent observation) from elsewhere.
- **Skills:** `TracingOptions::with_skills(discover_skills("skills"))` records the skills
  available to the application on the spans of scopes with
  `PropagatedAttributes::with_attach_skills(true)`; `with_skills_available(..)` sets them for one
  scope. `discover_skills` reads the skill directories (one `SKILL.md` each) and the
  `langfuse-skills.lock` versions like the Python SDK.
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

### `#[observe]` (feature `macros`)

```rust
use langfuse_rust_sdk::observe;
use langfuse_rust_sdk::otel::Observation;

#[observe(as_type = "retriever")]
async fn search(query: &str) -> Vec<String> { /* ... */ }

#[observe(name = "llm-call", as_type = "generation", skip(api_key))]
async fn complete(prompt: String, api_key: &str) -> Result<String, MyError> {
    if let Some(generation) = Observation::current() {
        generation.set_model("gpt-4o");
    }
    // ...
}

tracer.install_global();   // used when no observation is current
```

Each call is an observation, child of the current one, with the arguments (by name, `self`
left out) as input and the result as output; non-`Serialize` values are recorded as
`"<type name>"`. An `Err` result, a panic or a dropped future marks it as an error. A function
returning `impl Stream` or `impl Iterator` keeps its observation open until the stream ends.
Options: `name`, `as_type`, `capture_input`, `capture_output` (default from
`LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED`), `skip(..)`, `transform_to_string`.

### rig and other `tracing` instrumentation (feature `tracing-layer`)

```rust
use tracing_subscriber::prelude::*;

tracing_subscriber::registry().with(otel::tracing_layer(&tracer)).init();
```

[rig](https://docs.rs/rig-core) describes its LLM calls with `gen_ai.*` span fields: through the
layer they become generations with model, token usage and, with `record_content_telemetry(true)`,
input and output messages (verified against Langfuse Cloud). Spans started inside
`observation.in_scope(..)` are its children, and observations started inside an entered `tracing`
span are children of that span. rig-agent starts a new trace unless a `tracing` span is current:
run the agent inside one (`.instrument(span)`). By default only `gen_ai.*` spans of the layer are
exported; accept the others with `otel::is_tracing_layer_span`. See
[`rig_tracing`](integration-tests/examples/rig_tracing.rs).

### OpenAI (feature `openai`)

```rust
use langfuse_rust_sdk::openai::OpenAI;

let openai = OpenAI::new(async_openai::Client::new(), &tracer);
let response = openai.chat().with_name("summarize").create(request).await?;
let stream = openai.chat().create_stream(request).await?;    // recorded when it ends
let embedding = openai.embeddings().create(embedding_request).await?;
let answer = openai.responses().with_prompt(&prompt).create(response_request).await?;
```

Chat completions, the Responses API (with or without streaming) and embeddings are recorded like
in the Python SDK: messages, model parameters with their defaults, output, usage (Langfuse
computes the cost), time to first token for streams, and errors with level `ERROR`. Built for
`async-openai` 0.43.

### Amazon Bedrock (feature `bedrock`)

```rust
use langfuse_rust_sdk::bedrock::Bedrock;

let aws = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
let bedrock = Bedrock::new(aws_sdk_bedrockruntime::Client::new(&aws), &tracer);
let response = bedrock
    .converse(|r| r.model_id("us.anthropic.claude-sonnet-5-5").messages(message))
    .with_name("summarize")
    .send()
    .await?;
let mut stream = bedrock
    .converse_stream(|r| r.model_id(model).messages(message))
    .send()
    .await?;                                                  // recorded when it ends
while let Some(event) = stream.recv().await? { /* ... */ }
```

Converse and ConverseStream calls, for every model Bedrock serves (Claude, Nova, Llama, Mistral,
...), become generations: the system prompt and messages (images and documents as media), the
tools, the model ID, the inference parameters, the output message, the usage with prompt-cache
reads and writes (`input_cache_read`, `input_cache_creation`, priced by Langfuse), the stop
reason, time to first token for streams, and errors with level `ERROR`. See
[`bedrock_tracing`](integration-tests/examples/bedrock_tracing.rs).

### Usage of other providers

For calls traced by hand, `generation.set_provider_usage(&usage)` records the token usage a
provider returned in its own shape, translated like the Python SDK's LangChain integration does
(`usage::normalize_usage`): Anthropic (`input_tokens`, `cache_read_input_tokens`,
`cache_creation` tiers), Bedrock invocation metrics, Vertex AI / Gemini (`prompt_token_count`,
per-modality details), IBM, and LangChain-style `input_token_details`. Through the `tracing`
bridge, `gen_ai.usage.*` counts (recorded as strings by `tracing`) are exported as integers.

## AI Gateway (beta)

The [Langfuse AI Gateway](https://langfuse.com/docs/gateway/overview) forwards OpenAI and
Anthropic API calls with the organization's provider credentials and records each one as a
generation (usage, cost, latency; input and output in the `FULL` ingestion mode). Create a
gateway key under **AI Gateway → Gateway keys** and set `LANGFUSE_GATEWAY_KEY` and
`LANGFUSE_GATEWAY_HOST` (e.g. `https://gateway.us.cloud.langfuse.com`; without it, the host of
the Langfuse Cloud region of `LANGFUSE_BASE_URL` is used). No feature is needed:

```rust
use langfuse_rust_sdk::gateway::{GatewayClient, GatewayHeaders};
use serde_json::{Value, json};

let gateway = GatewayClient::from_env()?;
let models = gateway.anthropic_models::<Value>().await?.body; // IDs are `<credential>:<model>`

let reply = gateway
    .with_headers(GatewayHeaders::default().with_session_id("chat-42").with_tag("support"))
    .messages::<Value, _>(&json!({
        "model": "anthropic-prod:claude-sonnet-5-5",
        "max_tokens": 256,
        "messages": [{"role": "user", "content": "Say hello"}],
    }))
    .await?;
println!("{} (trace {:?})", reply.body["content"][0]["text"], reply.info.trace_id);

let mut stream = gateway.messages_stream(&body).await?;       // server-sent events
while let Some(event) = stream.next().await { let event = event?; /* event.json()? */ }
```

`responses`, `responses_compact`, `chat_completions` (and their `_stream` variants), `messages`,
`count_tokens` and the two model lists cover every route of the gateway; bodies are the
provider's own (`serde_json::Value` or your types). `GatewayResponseInfo` holds the gateway's
response headers: request ID, the trace and observation IDs of the recorded generation (to score
it), the ingestion project and the provider. `GatewayHeaders` sets user (service account keys),
session, environment, tags, metadata and trace name, percent-encoded and within the gateway's
1 KiB / 8 KiB limits. Model calls are retried only when the provider was not reached (408, 429,
503, connection failures), so a retry never records or bills a call twice; the default timeout
is 10 minutes.

**Trace context (feature `otel`).** A call made inside an observation sends its `traceparent`
and the attributes of `propagate_attributes` (user, session, environment, tags, metadata, trace
name; else the tracer's environment), so the gateway's generation becomes a child of that
observation. Wrap gateway calls in a span or an agent, not a generation, and do not also trace
them with `openai::OpenAI`: the gateway already records the call. `GatewayHeaders::current()`
gives the same headers for other HTTP clients. The SDK spans and the gateway's generation share
one trace only if the tracer's API keys belong to the gateway's ingestion project
(`GatewayResponseInfo::project_id`).

**async-openai (feature `openai`).** `gateway.openai_config()` is an `async_openai` configuration
for `{host}/openai/v1` with the gateway key that adds those headers to every request:

```rust
let openai = async_openai::Client::with_config(gateway.openai_config());
let response = span.in_scope(openai.responses().create(request)).await?; // child of `span`
```

See [`gateway`](examples/gateway.rs) and [`gateway_openai`](examples/gateway_openai.rs).

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
Those uploads run one at a time; `ConfigBuilder::media_upload_concurrency(n)` (or
`LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT`) runs up to `n` at once.

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
or a failed request returns a `BatchEvaluationResumeToken` to continue it. The run sends no
`fromStartTime`, so bound the time window in the filter with a `datetime` condition on
`startTime` (without a lower bound the API searches from 2020-01-01):

```rust
use langfuse_rust_sdk::batch_evaluation::BatchEvaluationOptions;

let mapper = |o: ObservationV2| async move {
    EvaluatorInput::new(o.input.unwrap_or_default(), o.output.unwrap_or_default())
};
let options = BatchEvaluationOptions::new(mapper)
    .with_evaluator(not_empty)
    .with_filter(r#"[
        {"type": "string", "column": "name", "operator": "=", "value": "chat"},
        {"type": "datetime", "column": "startTime", "operator": ">=", "value": "2026-11-01T00:00:00Z"}
    ]"#)
    .with_max_items(500);
let result = client.run_batched_evaluation(options).await?;
println!("{result}");
let next = result.resume_token;   // serializable; pass it to `with_resume_from`
```

## Blocking client (feature `blocking`)

```rust
use langfuse_rust_sdk::blocking::LangfuseClient;
use langfuse_rust_sdk::pagination::PageParams;

let client = LangfuseClient::new(ClientConfig::from_env()?)?;   // no #[tokio::main]
let prompt = client.prompts.get("summarizer")?;
for dataset in client.datasets.list_pages(PageParams::default()).items() {
    println!("{}", dataset?.name);
}
client.shutdown();   // send queued scores before exiting
```

Every API method has a blocking version, and `*_pages` methods return iterators. The client runs
the async SDK on a Tokio runtime of its own (one worker thread, which also sends queued scores and
exported spans), like `reqwest::blocking`; its methods panic when called inside an async runtime.
`client.tracer(..)` returns a tracer with blocking `flush` and `shutdown`, and
`client.block_on(..)` runs any async-only call.

## Pagination

Every list method returning `Paginated<T>` or `CursorPaginated<T>` has a `*_pages` companion
(`observations.list_pages`, `datasets.list_pages`, `annotation_queues.list_items_pages`, ...)
that returns a lazy stream of pages. `.items()` flattens it into a stream of items, and
`.take(n)` caps the number of items without requesting further pages. Each page request of
`observations.list_pages` has a 60 s timeout, as in the Python SDK (the server can scan for up
to 45 s); the timeout of a `with_options` view replaces it. Langfuse v4 recommends bounding every
observations query in time (without a lower bound the API searches from 2020-01-01), so the
example sets `from_start_time`:

```rust
use futures::TryStreamExt;
use langfuse_rust_sdk::api::observations::ListObservationsParams;

let params = ListObservationsParams {
    trace_id: Some("abc123".into()),
    from_start_time: Some("2026-11-01T00:00:00Z".into()),   // ISO 8601, inclusive
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
| other 4xx (402, 413, 422, ...) | `ClientError` |
| 503 | `ServiceUnavailable` |
| other 5xx | `ServerError` |

`LangfuseError` is `#[non_exhaustive]`, so `match` statements need a wildcard arm. `err.status()` returns the HTTP status, `err.message()` the server's message (the `message` or `error` field of a JSON body, the start of a body that is not JSON, such as a proxy's HTML page, or the status reason of an empty body) and `err.body()` the parsed JSON error body. The evaluators, evaluation-rules and unstable endpoints return a structured body that `PublicApiError::from_error(&err)` (or `UnstablePublicApiError::from_error`) parses.

Failed requests are retried with exponential backoff, honoring `Retry-After`, as long as repeating them cannot duplicate their effect:

| Failure | GET, HEAD, PUT, DELETE | POST, PATCH |
|---|---|---|
| 408, 429, 503 (request not processed) | retried | retried |
| Other 5xx | retried | not retried |
| Connection failure (request never sent) | retried | retried |
| Timeout or connection dropped mid-request | retried | not retried |

OTLP trace ingestion is retried like an idempotent request, since spans carry their IDs. 409 is never retried, because it signals a real conflict. See [`error_handling`](examples/error_handling.rs) for patterns.

## Requirements

- Rust 1.99+ (edition 2024)
- tokio runtime (or the blocking client)

## License

MIT
