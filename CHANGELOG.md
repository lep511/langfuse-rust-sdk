# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate uses
[Semantic Versioning](https://semver.org/) (pre-1.0: minor versions may break).

## [0.10.1] - 2026-11-10

### Fixed

- The `bedrock_tracing` example no longer sets `temperature`, which recent
  Claude models on Bedrock reject, and exports its trace (with the failed
  generation) when a call fails. It prints the question, records it as the
  input of its agent observation, and prints both answers.
- The default model of the example and of the `bedrock` docs is Claude
  Sonnet (`us.anthropic.claude-sonnet-5-5`). The integration was verified
  against Amazon Bedrock and Langfuse Cloud (model, usage, prompt-cache keys
  and cost).

## [0.10.0] - 2026-11-10

The main gaps found in a second comparison with the Python SDK (v5.2.0):
providers beyond OpenAI, prompt and observation timeouts, panics in
experiments, the span attribute limit, HTTP error details and credentials in
`Debug` output.

### Added

- **Amazon Bedrock integration** (feature `bedrock`, new optional
  dependencies `aws-sdk-bedrockruntime` and `aws-smithy-types`):
  `bedrock::Bedrock::new(client, &tracer)` records `converse(..).send()` and
  `converse_stream(..).send()` as generations, with the system prompt and
  messages (images and documents as media), tools, model ID, inference
  parameters, output, usage with prompt-cache reads and writes
  (`input_cache_read`, `input_cache_creation`), stop reason, latency, time to
  first token and errors. `TracedConverseStream` has `recv()` (like the SDK's
  `EventReceiver`) and `into_stream()`, and ends its generation when the
  stream ends, fails or is dropped. Per call: `with_name`, `with_metadata`,
  `with_prompt`, `with_trace_context`. Example `bedrock_tracing`.
- **`usage::normalize_usage`** (no feature needed): a port of the Python SDK's
  `_parse_usage_model`, translating the usage of Anthropic, Bedrock, Vertex AI
  / Gemini, IBM and LangChain-style details into Langfuse usage keys;
  `Observation::set_provider_usage` records it on a generation.
- `LangfuseError::status()` and `LangfuseError::message()`, and the variant
  `LangfuseError::ClientError { status, message, body }` for the 4xx statuses
  without a variant of their own (402, 408, 413, 422, ...).

### Changed

- `prompts.get_cached` without `fetch_timeout` now waits 5 s per attempt (the
  Python SDK default) unless the client timeout was chosen
  (`ConfigBuilder::timeout`, `LANGFUSE_TIMEOUT` or a custom `reqwest::Client`)
  or a `with_options` view sets one: a cold start returns the fallback in
  about 17 s instead of about 90 s when Langfuse does not answer.
- The SDK-built `reqwest::Client` has a 5 s connect timeout, so an
  unreachable host fails fast; the 30 s request timeout is unchanged.
- `observations.list_pages` requests each page with a 60 s timeout, as the
  Python SDK does (the server scans for up to 45 s); a view timeout replaces
  it.
- `run_experiment` and `run_batched_evaluation` catch panics in tasks,
  evaluators, composite and run evaluators and mappers (`futures-util` now
  uses its `std` feature): the item or evaluator counts as failed
  (`panicked: <message>`) and the run goes on, so its results are returned
  and its scores flushed.
- Observations keep their core attributes within the span attribute limit
  (`OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT`, default 128): metadata keys are dropped
  first, then the least important keys, with a warning. Before, the SDK
  dropped the attributes that sorted after the metadata keys (model, usage,
  output, prompt, user and session IDs) without a warning. Attributes equal
  to the ones the span started with are no longer written twice.
- **Breaking:** HTTP errors keep more detail. The message is the `message`
  field of a JSON body, else its `error` field, else the compact JSON, the
  start of a body that is not JSON (at most 500 characters) or the status
  reason of an empty body, instead of `Unknown error`. Other 4xx statuses
  are `ClientError` instead of `ServerError`. `RateLimited` has a `message`
  field.
- The tracing exporter turns `gen_ai.usage.*` counts recorded as strings
  (by the `tracing` bridge, e.g. for rig) back into integers.

### Fixed

- `{:?}` of `ClientConfig` no longer prints the `Authorization` header (the
  barely encoded secret key): it is marked sensitive.

## [0.9.0] - 2026-11-10

Loose ends of tracing and configuration (Phase 6 of the Python SDK parity
plan): compressed exports, span masking, a custom export path, tracer
injection, concurrent media uploads and the remaining environment variables.

### Added

- **Gzip-compressed span exports**, on by default as in the Python SDK:
  `TracingOptions::compression` (`otel::Compression::Gzip` or `None`,
  `with_compression`) sends `Content-Encoding: gzip` (new optional
  dependency `flate2`, part of the `otel` feature). `TracingOptions::from_env`
  reads `LANGFUSE_OTEL_COMPRESSION`, then `OTEL_EXPORTER_OTLP_TRACES_COMPRESSION`
  and `OTEL_EXPORTER_OTLP_COMPRESSION` (an invalid value falls through to the
  next one, then gzip). `max_batch_bytes` still counts uncompressed JSON.
  Verified against Langfuse Cloud.
- **`TracingOptions::with_mask_otel_spans(|spans| ..)`**: a hook over every
  export batch, after `should_export_span` and media upload, that edits span
  attributes through `otel::MaskableSpan` (`attribute`, `set_attribute`,
  `remove_attribute`, `attributes_mut`; IDs, name, parent, scope and resource
  are read-only). A panic drops the whole batch, so unmasked data is never
  sent. Type alias `otel::SpanMaskFn`.
- `TracingOptions::traces_export_path` (`with_traces_export_path`,
  `LANGFUSE_OTEL_TRACES_EXPORT_PATH`): export to another path under the base
  URL, e.g. behind a proxy.
- **Tracer injection:**
  - `TracingOptions::with_id_generator(..)`: an OpenTelemetry `IdGenerator`
    for the tracer's provider.
  - `TracingOptions::with_span_exporter(..)` (`otel::SharedSpanExporter`):
    send the spans to another OpenTelemetry `SpanExporter` instead of
    Langfuse; media upload, masking and batching still apply.
  - `otel::LangfuseSpanProcessor` is public: `LangfuseSpanProcessor::new(&client, options)`
    starts the export, a clone goes into a `SdkTracerProvider` you build (with
    your other processors and exporters), and
    `LangfuseTracer::with_provider(&processor, provider)` creates the tracer
    on it. Its `shutdown` stops the Langfuse export and leaves the provider
    running.
- **Concurrent media uploads**: `ConfigBuilder::media_upload_concurrency(n)`
  and `LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT` (default 1, as in the Python SDK);
  `ClientConfig::media_upload_concurrency()`. Flushing waits for every upload
  in flight.
- `TracingOptions::from_env` takes the release from the commit variable of
  common CI and hosting platforms when `LANGFUSE_RELEASE` is unset
  (`RENDER_GIT_COMMIT`, `CI_COMMIT_SHA`, `CIRCLE_SHA1`, `SOURCE_VERSION`,
  `TRAVIS_COMMIT`, `GIT_COMMIT`, `GITHUB_SHA`, `BITBUCKET_COMMIT`,
  `BUILD_SOURCEVERSION`, `DRONE_COMMIT_SHA`, in that order), and
  `OTEL_SDK_DISABLED=true` disables tracing, as the OpenTelemetry SDK under
  the Python SDK does. Creating a tracer while it is set logs a warning.
- Constants `otel::ENV_OTEL_COMPRESSION`, `otel::ENV_OTEL_TRACES_EXPORT_PATH`
  and `config::ENV_MEDIA_UPLOAD_THREAD_COUNT`.
- Documentation of debug logging (the Python SDK's `LANGFUSE_DEBUG`): the SDK
  logs through `tracing`, e.g. `RUST_LOG=langfuse_rust_sdk=debug` with an
  `EnvFilter` subscriber (README and crate docs).
- Example `otel_export_options` (`--features otel`): gzip export, a provider
  shared with another exporter and redaction with `mask_otel_spans`, checked
  through the API; the trace is deleted at the end.

### Changed

- Span exports are gzip-compressed unless `Compression::None` is set. Proxies
  in front of Langfuse must accept `Content-Encoding: gzip` (or set
  `LANGFUSE_OTEL_COMPRESSION=none`).
- `TracingOptions` has five new public fields; code that builds it with
  `Default::default().with_*(..)` (the documented way) is unaffected.

### Fixed

- The `api` module documentation no longer repeats its summary, and
  `cargo doc` builds without warnings (`redundant explicit link target`).

## [0.8.0] - 2026-11-10

Ergonomics and integrations: `#[observe]`, OpenAI and rig integrations, skills,
per-request options and a blocking client.

### Added

- **`#[observe]`** (feature `macros`, new crate `langfuse-rust-sdk-macros`):
  traces sync and async functions and methods as observations, children of
  the current one. Options `name`, `as_type` (any observation type but
  `event`; an invalid one is a compile error), `capture_input` /
  `capture_output` (default from
  `LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED`, read once), `skip(..)`,
  `transform_to_string` and `crate`. The input is an object by parameter name
  without `self`; values that are not `Serialize` are recorded as
  `"<type name>"` (decided at compile time). An `Err` result records level
  `ERROR`, the error's `Display` and an `exception` event; a panic or a
  dropped future (cancellation) also ends the observation with `ERROR`.
  Functions returning `impl Stream` / `impl Iterator` keep the observation
  open until the stream ends or is dropped, with the items (concatenated
  when they are strings) as output. The tracer is the current observation's,
  else the one of the new `LangfuseTracer::install_global` (`global`,
  `uninstall_global`); without either the function runs untraced.
- `otel::in_trace(TraceContext, future)` and `TraceContext::attach()`: attach
  the observations started in a scope to an existing trace and parent
  observation (marked as root like `start_observation_in_trace`).
  `Observation::record_error(&error)`.
- **OpenAI integration** (feature `openai`, `async-openai` 0.43):
  `openai::OpenAI::new(client, &tracer)` with `chat()` (`create`,
  `create_stream`), `responses()` (`create`, `create_stream`) and
  `embeddings()` (`create`), recorded like the Python SDK: input messages (the
  Responses API `instructions` as a system message, tools, audio inputs as data
  URIs), model parameters with the Python defaults (`max_tokens` `"Infinity"`,
  the response's `service_tier`, reasoning parameters), output, usage without
  nulls (`{"input": n}` for embeddings), the first chunk as
  `completion_start_time`, `finish_reason` and stream metadata, and errors
  (level `ERROR`, requested model, zero cost). Per call: `with_name`,
  `with_metadata`, `with_prompt`, `with_trace_context`. Verified against
  Langfuse Cloud (usage and cost mapped by the server).
- **Bridge from the `tracing` crate** (feature `tracing-layer`):
  `otel::tracing_layer(&tracer)` (`tracing-opentelemetry` 0.34 on the same
  `opentelemetry` 0.33), `otel::is_tracing_layer_span` and
  `TRACING_LAYER_SCOPE`. rig's `gen_ai.*` spans become generations with
  model, usage and messages, verified against Langfuse Cloud; context flows
  both ways between `tracing` spans and observations.
- **Skills**: `skills::discover_skills(dir)` (immediate subdirectories with a
  regular `SKILL.md`, symbolic links followed, versions from
  `langfuse-skills.lock` in the directory or the working directory, same rules
  and warnings as the Python SDK) and `SkillReference` (unknown fields kept).
  `TracingOptions::with_skills` configures the skills attached to the spans of
  scopes with `PropagatedAttributes::with_attach_skills(true)` (never to spans
  of another project); `PropagatedAttributes::with_skills_available` sets them
  for a scope (`[]` included, never as baggage). Attribute
  `langfuse.observation.skills_available`.
- **Per-request options**: `RequestOptions` (timeout, retries, headers, query
  and body parameters, and a `response_hook` that sees the status and headers
  of every response) and `LangfuseClient::with_options(options)`, a view of the
  client sharing its pool, prompt cache, project ID and queues. Views nest;
  retries keep the SDK's idempotency rules. `LangfuseClient::request_options`.
- **Blocking client** (feature `blocking`): `blocking::LangfuseClient` with a
  blocking version of every API method (generated by
  `scripts/generate_blocking.py`; a test checks none is missing), page
  iterators (`BlockingPages`, `.items()`), `score`, `flush`, `shutdown`,
  `with_options`, `block_on` and, with `otel`, a tracer with blocking `flush`
  and `shutdown`. It runs on an internal Tokio runtime with one worker
  thread.
- Examples `observe` (`--features macros`), `rig_tracing`
  (`--features tracing-layer`), `openai_tracing` (`--features openai`) and
  `blocking` (`--features blocking`, read-only).

### Changed

- Per-call headers (such as the ingestion version header) now replace default
  headers of the same name instead of being sent twice.

## [0.7.0] - 2026-11-09

Datasets with their items, experiments and batch evaluations.

### Added

- **Datasets**: `LangfuseClient::get_dataset(name, &GetDatasetOptions)` fetches
  a dataset with every item (`GET /api/public/v2/datasets/{name}`, then every
  page of `GET /api/public/dataset-items`) and returns a `DatasetWithItems`;
  `GetDatasetOptions` sets the page size (50) and the `version` to read the
  items at. Media stays as reference strings (each item's `media_references`
  has download URLs; `client.media.resolve_references` turns them into data
  URIs).
- `LangfuseClient::create_dataset_item(&CreateDatasetItemRequest)` uploads the
  media in `input`, `expected_output` and `metadata` (data URIs, which is how
  `LangfuseMedia` serializes, and the base64 shapes of LLM APIs) before
  creating the item, replacing it with reference strings. With media, a
  missing item ID is generated and the dataset is fetched once for its ID; a
  failed upload returns the error without creating the item.
  `CreateDatasetItemRequest::new(dataset_name)`.
- **`experiment` module**: `Evaluation` (`numeric`, `boolean`, `categorical`;
  the data type follows the `EvaluationValue`), the `Evaluator`,
  `CompositeEvaluator` and `RunEvaluator` traits with the `evaluator`,
  `composite_evaluator` and `run_evaluator` constructors for named async
  closures (returning an `Evaluation`, a `Vec`, an `Option` or a `Result` of
  them), the `Task` trait (async closures returning
  `Result<impl Serialize, impl Display>`; they may borrow), `ExperimentItem` /
  `LocalExperimentItem`, `ExperimentOptions`, `ExperimentResult` (with
  `format(include_item_results)`, `Display`, `{:#}` with the items, and
  `average(name)`) and `RegressionError` to fail CI on a regression.
- **Experiments** (`otel`): `LangfuseTracer::run_experiment(data, task,
  options)` and `DatasetWithItems::run_experiment(&tracer, task, options)` run
  the task on every item (`max_concurrency`, 50, at a time), each in an
  `experiment-item-run` trace with `experiment-item-task` (the task's scope)
  and `experiment-item-evaluation` (one `evaluator` span per evaluator)
  children, all with the `sdk-experiment` environment and the
  `langfuse.experiment.*` attributes Langfuse builds experiments from (no
  dataset run is created through the API). Item evaluations are scored on the
  task span, run evaluations on the experiment (`datasetRunId`), with the
  tracer's environment and sampling; spans and scores are flushed before
  returning. The experiment ID of dataset runs is derived from project,
  dataset and run name like the server does (same run name, same
  experiment); local items are identified by the hash of their input, as in
  the Python SDK. Failed items and evaluators are recorded and skipped, not
  fatal. Verified against Langfuse Cloud.
- `otel::attributes`: the ten `EXPERIMENT_*` keys and
  `SDK_EXPERIMENT_ENVIRONMENT`. `LangfuseTracer::is_enabled`.
- **Batch evaluation** (`batch_evaluation` module):
  `LangfuseClient::run_batched_evaluation(BatchEvaluationOptions)` reads
  observations from `GET /api/public/v2/observations` by cursor (JSON
  `filter`, `fields` plus `time`, `fetch_batch_size`, `max_items`, 3 retries
  and a 60 s timeout per page), deduplicates each page, maps every
  observation with a `Mapper` closure, runs the evaluators (`max_concurrency`,
  5) and queues one score per evaluation on the observation, with its trace,
  its environment and the run `metadata`. Returns a `BatchEvaluationResult`
  (totals, per-evaluator statistics, failed items, evaluations, `Display`);
  a run stopped by `max_items` or a failed page request carries a
  serializable `BatchEvaluationResumeToken` for `resume_from`.
- Examples `run_experiment` (`--features otel`; local data, checks the
  experiment is listed, then deletes its traces and scores) and
  `batch_evaluation` (evaluates a trace it ingests, then deletes it).
  `manage_datasets` upserts items with stable IDs through
  `create_dataset_item` and reads them back with `get_dataset`.

## [0.6.0] - 2026-11-09

Scores sent in the background, and media attachments.

### Breaking changes

- The option structs `api::prompts::GetPromptOptions`,
  `config::ScoreQueueOptions`, `otel::TracingOptions` and
  `otel::PropagatedAttributes` are `#[non_exhaustive]`, so later options no
  longer break your code. Struct literals (even with `..Default::default()`)
  no longer compile outside the crate: start from `Default::default()` (or
  `TracingOptions::from_env()`) and chain the new `with_*` methods, e.g.
  `GetPromptOptions::default().with_label("staging").with_cache_ttl(ttl)` or
  `PropagatedAttributes::default().with_user_id("u-1").with_tag("beta")`.
  Fields stay public for reading and assignment.

### Added

- **Score queue**: `LangfuseClient::score(CreateScoreRequest)` queues a score
  without blocking; a Tokio task sends batches to `POST /api/public/scores`
  every `flush_interval` (1 s) or `flush_at` (15) scores, under 1,000,000
  bytes per request (larger scores are dropped). Missing IDs are generated, so
  failed batches are retried without duplicates; partial rejections (207) are
  logged. `LangfuseClient::flush` / `shutdown` wait for the queue.
  `ConfigBuilder::score_queue(ScoreQueueOptions)`, and `from_env()` reads
  `LANGFUSE_FLUSH_AT`, `LANGFUSE_FLUSH_INTERVAL` and
  `LANGFUSE_MAX_BATCH_SIZE_BYTES`.
- **Scores from tracing** (`otel`): `Observation::score` / `score_trace` (with
  the observation's environment; dropped for unsampled traces) and
  `LangfuseTracer::score_current_span` / `score_current_trace`.
  `LangfuseTracer::score` queues any score with the tracer's sampling: scores
  of sampled-out traces are dropped (the decision depends only on the trace
  ID, as for spans), scores without a W3C trace ID are always sent.
  `LangfuseClient::score` does not sample.
- **Media** (`media` module): `LangfuseMedia` from bytes, a file or a base64
  data URI, with its SHA-256, media ID, reference string
  (`@@@langfuseMedia:type=..|id=..|source=..@@@`) and data URI;
  `LangfuseMedia::upload(&client, &MediaTarget)` requests the presigned URL,
  uploads (retrying network errors, 429 and 5xx) and reports the outcome;
  `MediaReference::parse`; `MediaClient::download` and
  `MediaClient::resolve_references`, which turns references in fetched data
  back into data URIs. `LangfuseMedia` serializes as a data URI.
- **Automatic media upload** (`otel`): base64 data URIs and the base64 shapes
  of common LLM APIs in exported span attributes (of any instrumentation) are
  uploaded in the background and replaced by reference strings.
  `TracingOptions::media_upload` / `LANGFUSE_MEDIA_UPLOAD_ENABLED` turn it off.
- `MediaContentType::from_mime` / `mime`, and `Deserialize` for it.
- `LangfuseError::Media` for invalid media and failed uploads or downloads.
- Example `media` (`--features otel`); `otel_tracing` scores its generation.

### Changed

- `LangfuseTracer::flush` and `shutdown` also flush the client's scores and
  media uploads, after the spans: exporting a span queues the uploads of its
  media, so stopping at the spans would leave references to files not
  uploaded yet. `shutdown` does not stop the client; `LangfuseClient::shutdown`
  does.

## [0.5.0] - 2026-11-09

Native tracing on OpenTelemetry, behind the new `otel` feature. Without the
feature nothing changes.

### Added

- **`otel` feature** (depends on `opentelemetry` / `opentelemetry_sdk` 0.33,
  re-exported as `otel::opentelemetry` and `otel::opentelemetry_sdk`).
- `LangfuseClient::tracer(TracingOptions)` / `LangfuseTracer::new` create a
  tracer with its own `SdkTracerProvider`: spans are queued and exported in the
  background through `IngestionClient` (same authentication, retries and HTTP
  client), every `flush_interval` (5 s) or `flush_at` (512) spans, split to stay
  under `max_batch_bytes` (1 MiB). `flush().await` and `shutdown().await` wait
  for the export. `provider()` exposes the provider for other instrumentation.
- `Observation` (from `tracer.start_observation(name, ObservationType)`), with
  10 types: span, generation, event, agent, tool, chain, retriever, embedding,
  evaluator and guardrail. Setters `set_input`, `set_output`, `set_metadata`
  (merged per key), `set_version`, `set_level`, `set_status_message`,
  `set_name`, `set_trace_as_public` and, for generations and embeddings,
  `set_model`, `set_model_parameters`, `set_usage_details`,
  `set_cost_details`, `set_completion_start_time` and `set_prompt`. Setters
  never fail and the attributes are written once when the observation ends
  (`end()`, or when the last handle drops). `Level::Error` marks the span as
  failed; events end where they start.
- Context: `Observation::in_scope(future)` makes an observation current
  (children, `Observation::current()`, `otel::current_trace_id()`,
  `otel::current_observation_id()`); `start_child` inherits the attributes of
  the current scope.
- `otel::propagate_attributes(PropagatedAttributes, future)`: user, session,
  tags, metadata, version, trace name, environment and prompt for every span of
  a scope, validated and merged across nested scopes, optionally as W3C
  baggage (`as_baggage`), which is also read back from incoming context.
- `TraceContext` + `tracer.start_observation_in_trace` attach observations to
  an existing trace (and parent observation).
- `TracingOptions` (`from_env()` reads `LANGFUSE_TRACING_ENVIRONMENT`,
  `LANGFUSE_RELEASE`, `LANGFUSE_SAMPLE_RATE`, `LANGFUSE_TRACING_ENABLED`,
  `LANGFUSE_FLUSH_AT`, `LANGFUSE_FLUSH_INTERVAL`,
  `LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES`): environment and release, trace-ID
  ratio sampling, `enabled = false` for no-ops, `mask` for input, output and
  metadata, and `should_export_span` with the default filter
  `is_default_export_span` (Langfuse observations, `gen_ai.*` spans and known
  LLM instrumentation scopes). The first exported span of the application in
  each trace is marked as its root.
- `otel::attributes` exposes the attribute keys Langfuse reads.
- `LangfuseClient::public_key()`.
- Example `otel_tracing` (`--features otel`).

## [0.4.1] - 2026-11-09

### Added

- `Prompt::compile_with_json_placeholders(&variables, placeholders)`: placeholder
  values can be any type that serializes to an array of JSON objects (such as
  the message types of LLM client crates), and every field is kept
  (`tool_calls`, `tool_call_id`, `name`, ...). Returns
  `CompiledPrompt<serde_json::Value>`.
- `BasePrompt` implements `Default`, so mocks can use
  `BasePrompt { name: .., ..Default::default() }` without breaking when fields
  are added.

### Changed

- `CompiledPrompt` takes a defaulted message type parameter
  (`CompiledPrompt<M = ChatPromptMessage>`); existing code is unaffected.
- Retries now depend on whether a request can be repeated safely:
  - connection failures (the request was never sent) are retried for every
    method; timeouts and connections dropped mid-request only for GET, HEAD,
    PUT and DELETE;
  - POST and PATCH are retried only on 408, 429 and 503, no longer on other
    5xx, which could duplicate their effect (e.g. a second prompt version);
  - OTLP trace ingestion is retried like an idempotent request, since spans
    carry their IDs.

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
