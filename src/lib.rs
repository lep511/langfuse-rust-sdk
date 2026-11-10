//! Async Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.
//!
//! Built on [`reqwest`] + [`tokio`], the SDK exposes one sub-client per API
//! resource group through [`LangfuseClient`].  All sub-clients share a
//! single connection pool, authentication header and retry policy.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), LangfuseError> {
//!     // Build the client — keys come from the Langfuse project settings.
//!     let config = ClientConfig::builder("pk-lf-…", "sk-lf-…")
//!         .base_url("https://cloud.langfuse.com")
//!         .build()?;
//!     let client = LangfuseClient::new(config)?;
//!
//!     // Verify connectivity.
//!     let health = client.health.check().await?;
//!     println!("Langfuse {} is {}", health.version, health.status);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Configuration
//!
//! [`ClientConfig::from_env`] reads `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`,
//! `LANGFUSE_BASE_URL` and `LANGFUSE_TIMEOUT`:
//!
//! ```rust,no_run
//! # use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
//! # fn main() -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let client = LangfuseClient::new(ClientConfig::from_env()?)?;
//! # Ok(())
//! # }
//! ```
//!
//! [`ClientConfig::builder`] accepts the public and secret keys and returns a
//! [`ConfigBuilder`] with sensible defaults.  Override them as needed:
//!
//! ```rust,no_run
//! # use langfuse_rust_sdk::{ClientConfig, RetryPolicy};
//! # use std::time::Duration;
//! let config = ClientConfig::builder("pk-lf-…", "sk-lf-…")
//!     .base_url("https://self-hosted.example.com")
//!     .timeout(Duration::from_secs(60))
//!     .retry_policy(RetryPolicy {
//!         max_retries: 5,
//!         ..Default::default()
//!     })
//!     .header("x-custom", "value")
//!     .build()
//!     .expect("valid config");
//! ```
//!
//! # Scores and media
//!
//! [`LangfuseClient::score`] queues scores that a background task sends in
//! batches; [`media::LangfuseMedia`] uploads files and builds the reference
//! strings Langfuse uses for them.
//!
//! # Tracing
//!
//! With the `otel` feature, `LangfuseClient::tracer` returns an
//! `otel::LangfuseTracer` that records observations (spans, generations,
//! agents, tools, ...) as OpenTelemetry spans and exports them in the
//! background; see the `otel` module.
//!
//! # Datasets, experiments and evaluations
//!
//! [`LangfuseClient::get_dataset`] fetches a dataset with all its items and
//! [`LangfuseClient::create_dataset_item`] uploads the media of a new item.
//! With the `otel` feature, `LangfuseTracer::run_experiment` runs a task on
//! dataset items or local data, traces and evaluates every run with the
//! evaluators of [`experiment`], and returns an [`experiment::ExperimentResult`].
//! [`LangfuseClient::run_batched_evaluation`] scores observations already
//! stored in Langfuse with the same evaluators (see [`batch_evaluation`]).
//!
//! # Logging
//!
//! The SDK logs through the [`tracing`] crate (the Python SDK's
//! `LANGFUSE_DEBUG`): requests, retries and exports at `debug`, dropped data
//! and failures at `warn` and `error`. Nothing is printed until the
//! application installs a subscriber, e.g. `tracing_subscriber::fmt()` with
//! an `EnvFilter` and `RUST_LOG=langfuse_rust_sdk=debug`.
//!
//! # Error handling
//!
//! Every method returns `Result<T, `[`LangfuseError`]`>`.  HTTP status codes
//! are mapped to dedicated variants so callers can match on specific failure
//! modes:
//!
//! ```rust,no_run
//! # use langfuse_rust_sdk::{LangfuseClient, LangfuseError, ClientConfig};
//! # async fn example(client: &LangfuseClient) {
//! match client.health.check().await {
//!     Ok(h) => println!("ok: {}", h.status),
//!     Err(LangfuseError::Unauthorized { .. }) => eprintln!("bad API keys"),
//!     Err(LangfuseError::RateLimited { retry_after, .. }) => {
//!         eprintln!("throttled, retry after {retry_after:?}");
//!     }
//!     Err(e) => eprintln!("other error: {e}"),
//! }
//! # }
//! ```
#![warn(missing_docs)]
// Documented by its inner `//!` docs: outer docs here would be merged with
// them and resolved in this module's scope.
pub mod api;
/// SDK configuration and builder.
pub mod config;
/// Error types returned by every SDK method.
pub mod error;
/// Low-level HTTP transport shared by all API clients.
pub mod http;
/// Trace and observation ID generation (random or derived from a seed).
pub mod ids;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
pub mod media;
/// Request and response model structs generated from the OpenAPI spec.
pub mod models;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
mod background;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
pub mod batch_evaluation;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
#[cfg(feature = "blocking")]
pub mod blocking;
mod dataset;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
pub mod experiment;
#[cfg(feature = "otel")]
pub mod otel;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
#[cfg(feature = "bedrock")]
pub mod bedrock;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
#[cfg(feature = "openai")]
pub mod openai;
/// Pagination helpers for list endpoints.
pub mod pagination;
mod prompt_cache;
/// Retry policy with exponential backoff and jitter.
pub mod retry;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
pub mod skills;
/// Standalone utilities (YAML processing).
pub mod tools;
// Documented by its own `//!` docs, so intra-doc links resolve inside the module.
pub mod usage;

/// Trace a function as a Langfuse observation (feature `macros`).
///
/// Every call becomes an observation named after the function, with its
/// arguments as input and its result as output. The observation is current
/// while the function runs, so observations started inside (by other
/// observed functions, `tracer.start_observation` or other
/// instrumentation) are its children, and [`Observation::current`](otel::Observation::current)
/// returns it, for example to set the model of a generation.
///
/// ```rust,no_run
/// use langfuse_rust_sdk::observe;
/// use langfuse_rust_sdk::otel::{Observation, TracingOptions};
/// use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
/// use serde_json::json;
///
/// #[observe(as_type = "retriever")]
/// async fn search(query: &str, limit: usize) -> Vec<String> {
///     vec![format!("result for {query}"); limit]
/// }
///
/// #[observe(name = "llm-call", as_type = "generation")]
/// async fn complete(prompt: String) -> Result<String, std::io::Error> {
///     if let Some(generation) = Observation::current() {
///         generation.set_model("gpt-4o").set_usage_details(&json!({"input": 12, "output": 3}));
///     }
///     Ok(format!("answer to {prompt}"))
/// }
///
/// #[observe]
/// async fn answer(question: &str) -> Result<String, std::io::Error> {
///     let context = search(question, 3).await;
///     complete(format!("{question} {}", context.join(" "))).await
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = LangfuseClient::new(ClientConfig::from_env()?)?;
/// let tracer = client.tracer(TracingOptions::from_env()?)?;
/// tracer.install_global(); // used when no observation is current
///
/// answer("Where is my order?").await?; // one trace: answer → search, llm-call
/// tracer.shutdown().await;
/// # Ok(())
/// # }
/// ```
///
/// # Options
///
/// `#[observe(name = "...", as_type = "...", capture_input = bool, capture_output = bool,
/// skip(arg, ...), transform_to_string = path, crate = "path")]`, all optional:
///
/// - `name`: observation name (default: the function name).
/// - `as_type`: `span` (default), `generation`, `agent`, `tool`, `chain`,
///   `retriever`, `embedding`, `evaluator` or `guardrail`; anything else
///   (including `event`, which has no duration) is a compile error.
/// - `capture_input` / `capture_output`: record the arguments / the result.
///   By default both are recorded unless the
///   `LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED` environment variable is
///   `false` or `0` (read once, at the first call).
/// - `skip(a, b)`: arguments never recorded (e.g. secrets or clients).
/// - `transform_to_string`: for streams and iterators, a
///   `fn(&[serde_json::Value]) -> String` that builds the output from the
///   recorded items.
/// - `crate`: path of this crate when it is renamed in `Cargo.toml`.
///
/// # What is recorded
///
/// - **Input**: an object with one entry per argument, by parameter name
///   (`self` is left out). The Python SDK records `{"args": [...], "kwargs": {...}}`;
///   Rust has no keyword arguments. Functions without arguments record no input.
/// - **Values**: arguments, results and stream items are recorded as JSON
///   when their type implements `Serialize`, and as `"<type name>"`
///   otherwise (decided at compile time). The tracer's
///   [`mask`](otel::TracingOptions::mask) applies.
/// - **Errors**: when the function returns a `Result` and it is `Err`, the
///   observation gets level `ERROR`, the error's `Display` (or its type name
///   if that is empty) as status message, and an `exception` event (see
///   [`Observation::record_error`](otel::Observation::record_error)); `Ok(v)`
///   records `v` as output. A panic, or a future dropped before it finishes
///   (cancellation), also ends the observation with level `ERROR`.
/// - **Streams and iterators**: a function returning `impl Stream<Item = T>`
///   or `impl Iterator<Item = T>` (with `Send`, `Sync`, `Unpin` or lifetime
///   bounds at most) keeps its observation open until the stream or iterator
///   ends or is dropped, and the observation is current while it produces
///   items. The output is the `transform_to_string` result, else the
///   concatenation when every item is a string, else the list of items; an
///   `Err` item marks the observation as failed.
///
/// # Which tracer
///
/// Inside an observation, the observed function uses that observation's
/// tracer; otherwise the global one of
/// [`LangfuseTracer::install_global`](otel::LangfuseTracer::install_global).
/// Without either the function runs untraced, at the cost of a context
/// lookup. Run a call inside [`otel::in_trace`] to attach it to an existing
/// trace or parent observation (the Python decorator's `langfuse_trace_id`
/// and `langfuse_parent_observation_id` arguments).
///
/// Synchronous functions work too: the observation is current until they
/// return (for code on the same thread). Like every context in async Rust,
/// the observation does not follow `tokio::spawn`: wrap the spawned future
/// with [`Observation::in_scope`](otel::Observation::in_scope).
#[cfg(feature = "macros")]
pub use langfuse_rust_sdk_macros::observe;

pub use config::{ClientConfig, ConfigBuilder};
pub use error::LangfuseError;
pub use http::RequestOptions;
pub use pagination::{CursorPaginated, Paginated};
pub use retry::RetryPolicy;

use std::sync::{Arc, OnceLock};

use crate::api::annotation_queues::AnnotationQueuesClient;
use crate::api::blob_storage_integrations::BlobStorageIntegrationsClient;
use crate::api::comments::CommentsClient;
use crate::api::dataset_items::DatasetItemsClient;
use crate::api::datasets::DatasetsClient;
use crate::api::evaluation_rules::EvaluationRulesClient;
use crate::api::evaluators::EvaluatorsClient;
use crate::api::experiments::ExperimentsClient;
use crate::api::feedback::FeedbackClient;
use crate::api::health::HealthClient;
use crate::api::ingestion::IngestionClient;
use crate::api::llm_connections::LlmConnectionsClient;
use crate::api::media::MediaClient;
use crate::api::metrics::MetricsClient;
use crate::api::models::ModelsClient;
use crate::api::observations::ObservationsClient;
use crate::api::organizations::OrganizationsClient;
use crate::api::projects::ProjectsClient;
use crate::api::prompt_version::PromptVersionClient;
use crate::api::prompts::PromptsClient;
use crate::api::scim::ScimClient;
use crate::api::score_configs::ScoreConfigsClient;
use crate::api::scores::ScoresClient;
use crate::api::skills::SkillsClient;
use crate::api::traces::TracesClient;
#[cfg(feature = "unstable")]
use crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient;
#[cfg(feature = "unstable")]
use crate::api::unstable_dashboards::UnstableDashboardsClient;
#[cfg(feature = "unstable")]
use crate::api::unstable_topics::UnstableTopicsClient;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::projects::Project;

/// Entry point of the SDK: one sub-client per API resource group, all sharing
/// one HTTP client (connection pool, auth, retries).
///
/// Cloning is cheap: clones share the connection pool and configuration, so a
/// clone can be moved into each spawned task.
#[derive(Clone)]
#[non_exhaustive]
pub struct LangfuseClient {
    /// Health-check endpoint.
    pub health: HealthClient,
    /// Create, list, update and delete scores.
    pub scores: ScoresClient,
    /// Manage score configuration presets.
    pub score_configs: ScoreConfigsClient,
    /// Query observations (spans, generations, events).
    pub observations: ObservationsClient,
    /// Create and manage datasets.
    pub datasets: DatasetsClient,
    /// Create and manage dataset items.
    pub dataset_items: DatasetItemsClient,
    /// Retrieve and manage prompts.
    pub prompts: PromptsClient,
    /// Manage individual prompt versions.
    pub prompt_versions: PromptVersionClient,
    /// CRUD operations on LLM model definitions.
    pub models: ModelsClient,
    /// Manage skills.
    pub skills: SkillsClient,
    /// Project-level operations.
    pub projects: ProjectsClient,
    /// Create and query traces.
    pub traces: TracesClient,
    /// Batch ingestion of traces, spans, generations, scores and events.
    pub ingestion: IngestionClient,
    /// Upload and manage media assets.
    pub media: MediaClient,
    /// Manage comments on traces and observations.
    pub comments: CommentsClient,
    /// Submit end-user feedback.
    pub feedback: FeedbackClient,
    /// Manage annotation queues and their items.
    pub annotation_queues: AnnotationQueuesClient,
    /// Blob storage integration configuration.
    pub blob_storage_integrations: BlobStorageIntegrationsClient,
    /// Manage automated evaluation rules.
    pub evaluation_rules: EvaluationRulesClient,
    /// Manage evaluator definitions.
    pub evaluators: EvaluatorsClient,
    /// Manage experiments and their runs.
    pub experiments: ExperimentsClient,
    /// Manage LLM provider connections.
    pub llm_connections: LlmConnectionsClient,
    /// Query aggregated metrics.
    pub metrics: MetricsClient,
    /// Organization-level operations.
    pub organizations: OrganizationsClient,
    /// SCIM 2.0 user provisioning.
    pub scim: ScimClient,
    /// *(unstable)* Dashboard widget queries.
    #[cfg(feature = "unstable")]
    pub unstable_dashboard_widgets: UnstableDashboardWidgetsClient,
    /// *(unstable)* Dashboard CRUD.
    #[cfg(feature = "unstable")]
    pub unstable_dashboards: UnstableDashboardsClient,
    /// *(unstable)* Topic management.
    #[cfg(feature = "unstable")]
    pub unstable_topics: UnstableTopicsClient,
    http: Arc<HttpClient>,
    /// Project of the API keys, resolved once by [`Self::auth_check`].
    project_id: Arc<OnceLock<String>>,
    public_key: Arc<str>,
    /// Score queue and media uploads, shared by clones.
    background: Arc<background::Background>,
}

impl LangfuseClient {
    /// Create a new client from the given configuration.
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let prompt_cache = Arc::new(prompt_cache::PromptCache::new(config.prompt_cache_ttl));
        let public_key: Arc<str> = Arc::from(config.public_key.as_str());
        let score_queue = config.score_queue.clone();
        let media_upload_concurrency = config.media_upload_concurrency;
        let http = Arc::new(HttpClient::new(config)?);
        let background = Arc::new(background::Background::new(
            Arc::clone(&http),
            score_queue,
            media_upload_concurrency,
        ));
        Ok(Self::assemble(
            http,
            prompt_cache,
            Arc::new(OnceLock::new()),
            public_key,
            background,
        ))
    }

    /// A view of this client that applies `options` to every API request:
    /// timeout, retries, extra headers, query and body parameters, or a hook
    /// that sees the response headers (see [`RequestOptions`]).
    ///
    /// The view shares everything else with this client: connection pool,
    /// prompt cache, cached project ID and background queues. Scores queued
    /// with [`score`](Self::score) and media uploads keep the options of the
    /// client that created the queues, the original one. Views nest: the
    /// options of `client.with_options(a).with_options(b)` are `a` overridden
    /// by `b`. Creating a view is cheap (no I/O); keep it for repeated use.
    ///
    /// ```rust,no_run
    /// use langfuse_rust_sdk::{LangfuseClient, RequestOptions};
    /// use std::time::Duration;
    ///
    /// # async fn example(client: &LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
    /// let fast = client.with_options(
    ///     RequestOptions::default()
    ///         .with_timeout(Duration::from_secs(2))
    ///         .with_max_retries(0),
    /// );
    /// let health = fast.health.check().await?;
    /// # let _ = health;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_options(&self, options: RequestOptions) -> LangfuseClient {
        Self::assemble(
            Arc::new(self.http.with_options(&options)),
            Arc::clone(&self.prompts.cache),
            Arc::clone(&self.project_id),
            Arc::clone(&self.public_key),
            Arc::clone(&self.background),
        )
    }

    /// The overrides applied to every request of this client (empty unless
    /// it is a view from [`with_options`](Self::with_options)).
    pub fn request_options(&self) -> &RequestOptions {
        self.http.options()
    }

    fn assemble(
        http: Arc<HttpClient>,
        prompt_cache: Arc<prompt_cache::PromptCache>,
        project_id: Arc<OnceLock<String>>,
        public_key: Arc<str>,
        background: Arc<background::Background>,
    ) -> Self {
        Self {
            health: HealthClient {
                http: Arc::clone(&http),
            },
            scores: ScoresClient {
                http: Arc::clone(&http),
            },
            score_configs: ScoreConfigsClient {
                http: Arc::clone(&http),
            },
            observations: ObservationsClient {
                http: Arc::clone(&http),
            },
            datasets: DatasetsClient {
                http: Arc::clone(&http),
            },
            dataset_items: DatasetItemsClient {
                http: Arc::clone(&http),
            },
            prompts: PromptsClient {
                http: Arc::clone(&http),
                cache: Arc::clone(&prompt_cache),
            },
            prompt_versions: PromptVersionClient {
                http: Arc::clone(&http),
                cache: prompt_cache,
            },
            models: ModelsClient {
                http: Arc::clone(&http),
            },
            skills: SkillsClient {
                http: Arc::clone(&http),
            },
            projects: ProjectsClient {
                http: Arc::clone(&http),
            },
            traces: TracesClient {
                http: Arc::clone(&http),
            },
            ingestion: IngestionClient {
                http: Arc::clone(&http),
            },
            media: MediaClient {
                http: Arc::clone(&http),
            },
            comments: CommentsClient {
                http: Arc::clone(&http),
            },
            feedback: FeedbackClient {
                http: Arc::clone(&http),
            },
            annotation_queues: AnnotationQueuesClient {
                http: Arc::clone(&http),
            },
            blob_storage_integrations: BlobStorageIntegrationsClient {
                http: Arc::clone(&http),
            },
            evaluation_rules: EvaluationRulesClient {
                http: Arc::clone(&http),
            },
            evaluators: EvaluatorsClient {
                http: Arc::clone(&http),
            },
            experiments: ExperimentsClient {
                http: Arc::clone(&http),
            },
            llm_connections: LlmConnectionsClient {
                http: Arc::clone(&http),
            },
            metrics: MetricsClient {
                http: Arc::clone(&http),
            },
            organizations: OrganizationsClient {
                http: Arc::clone(&http),
            },
            scim: ScimClient {
                http: Arc::clone(&http),
            },
            #[cfg(feature = "unstable")]
            unstable_dashboard_widgets: UnstableDashboardWidgetsClient {
                http: Arc::clone(&http),
            },
            #[cfg(feature = "unstable")]
            unstable_dashboards: UnstableDashboardsClient {
                http: Arc::clone(&http),
            },
            #[cfg(feature = "unstable")]
            unstable_topics: UnstableTopicsClient {
                http: Arc::clone(&http),
            },
            http,
            project_id,
            public_key,
            background,
        }
    }

    /// Queue a score; it is sent in the background with others in one request
    /// (see [`ConfigBuilder::score_queue`]). Never blocks and never fails:
    /// problems are logged through `tracing`.
    ///
    /// A missing `id` is generated (so a retried batch does not duplicate the
    /// score) and a missing `timestamp` is set to now. The first call must
    /// happen inside a Tokio runtime, which runs the sending task. Call
    /// [`flush`](Self::flush) (or [`shutdown`](Self::shutdown)) before the
    /// program exits. Use [`scores.create`](ScoresClient::create) to send
    /// immediately and get the response.
    ///
    /// The client does not know about tracing: with a sampling tracer
    /// (feature `otel`), score through `LangfuseTracer::score`, which drops
    /// the scores of sampled-out traces.
    pub fn score(&self, mut score: models::scores::CreateScoreRequest) {
        if score.id.is_none() {
            score.id = Some(ids::create_observation_id(None));
        }
        if score.timestamp.is_none() {
            score.timestamp = Some(chrono::Utc::now());
        }
        self.background.enqueue_score(score);
    }

    /// Send every queued score and finish every queued media upload.
    pub async fn flush(&self) {
        self.background.flush().await;
    }

    /// Flush, then stop the background tasks: later scores and media uploads
    /// are dropped. Affects every clone of this client.
    pub async fn shutdown(&self) {
        self.background.shutdown().await;
    }

    /// Upload `media` in the background (used by the `otel` exporter).
    #[cfg_attr(not(feature = "otel"), allow(dead_code))]
    pub(crate) fn queue_media_upload(
        &self,
        media: media::LangfuseMedia,
        target: media::MediaTarget,
    ) {
        self.background.enqueue_media(media, target);
    }

    /// The public API key of this client.
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    /// Create a [`LangfuseTracer`](otel::LangfuseTracer) exporting through this
    /// client (feature `otel`). Must be called inside a Tokio runtime.
    #[cfg(feature = "otel")]
    pub fn tracer(
        &self,
        options: otel::TracingOptions,
    ) -> Result<otel::LangfuseTracer, LangfuseError> {
        otel::LangfuseTracer::new(self, options)
    }

    /// Verify the API keys and return the project they belong to.
    ///
    /// Calls `GET /api/public/projects`; an [`Unauthorized`](LangfuseError::Unauthorized)
    /// error means the keys are wrong, and an empty project list returns
    /// [`LangfuseError::Config`]. The project ID is cached for
    /// [`trace_url`](Self::trace_url). Avoid calling this on hot paths: it is
    /// one network round trip.
    pub async fn auth_check(&self) -> Result<Project, LangfuseError> {
        let project = self
            .projects
            .list()
            .await?
            .data
            .into_iter()
            .next()
            .ok_or_else(|| {
                LangfuseError::Config(
                    "Auth check failed: no project found for the provided API keys".into(),
                )
            })?;
        // Another task may have cached it already; both values are the same project.
        let _ = self.project_id.set(project.id.clone());
        Ok(project)
    }

    /// URL of a trace in the Langfuse UI:
    /// `{base_url}/project/{project_id}/traces/{trace_id}`.
    ///
    /// The project ID is fetched with [`auth_check`](Self::auth_check) on the
    /// first call and cached (shared by clones of this client).
    pub async fn trace_url(&self, trace_id: &str) -> Result<String, LangfuseError> {
        let project_id = self.resolve_project_id().await?;
        Ok(format!(
            "{}/project/{}/traces/{}",
            self.ui_base_url(),
            encode_path_segment(&project_id),
            encode_path_segment(trace_id)
        ))
    }

    /// The project ID of the API keys, cached after the first [`auth_check`](Self::auth_check).
    pub(crate) async fn resolve_project_id(&self) -> Result<String, LangfuseError> {
        match self.project_id.get() {
            Some(id) => Ok(id.clone()),
            None => Ok(self.auth_check().await?.id),
        }
    }

    /// The base URL without its trailing `/`, for links to the Langfuse UI.
    pub(crate) fn ui_base_url(&self) -> &str {
        self.http.base_url().as_str().trim_end_matches('/')
    }
}
