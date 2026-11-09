//! Async Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.
//!
//! Built on `reqwest` + `tokio`, the SDK exposes one sub-client per API
//! resource group through [`LangfuseClient`].  All sub-clients share a
//! single connection pool, authentication header and retry policy.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use langfuse_sdk::{ClientConfig, LangfuseClient};
//!
//! # async fn run() -> Result<(), langfuse_sdk::LangfuseError> {
//! let config = ClientConfig::builder("pk-lf-…", "sk-lf-…")
//!     .base_url("https://cloud.langfuse.com")
//!     .build()?;
//! let client = LangfuseClient::new(config)?;
//! let health = client.health.check().await?;
//! # Ok(())
//! # }
//! ```
#![warn(missing_docs)]
/// API client modules — one per resource group.
pub mod api;
/// SDK configuration and builder.
pub mod config;
/// Error types returned by every SDK method.
pub mod error;
/// Low-level HTTP transport shared by all API clients.
pub mod http;
/// Request and response model structs generated from the OpenAPI spec.
pub mod models;
/// Pagination helpers for list endpoints.
pub mod pagination;
/// Retry policy with exponential backoff and jitter.
pub mod retry;
/// Standalone utilities (YAML processing).
pub mod tools;

pub use config::{ClientConfig, ConfigBuilder};
pub use error::LangfuseError;
pub use pagination::{CursorPaginated, Paginated};
pub use retry::RetryPolicy;

use std::sync::Arc;

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
use crate::http::HttpClient;

/// Entry point of the SDK: one sub-client per API resource group, all sharing
/// one HTTP client (connection pool, auth, retries).
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
}

impl LangfuseClient {
    /// Create a new client from the given configuration.
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let http = Arc::new(HttpClient::new(config)?);
        Ok(Self {
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
            },
            prompt_versions: PromptVersionClient {
                http: Arc::clone(&http),
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
        })
    }
}
