#![warn(missing_docs)]
pub mod api;
pub mod config;
pub mod error;
pub mod http;
pub mod models;
pub mod pagination;
pub mod retry;
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
    pub health: HealthClient,
    pub scores: ScoresClient,
    pub score_configs: ScoreConfigsClient,
    pub observations: ObservationsClient,
    pub datasets: DatasetsClient,
    pub dataset_items: DatasetItemsClient,
    pub prompts: PromptsClient,
    pub prompt_versions: PromptVersionClient,
    pub models: ModelsClient,
    pub skills: SkillsClient,
    pub projects: ProjectsClient,
    pub traces: TracesClient,
    pub ingestion: IngestionClient,
    pub media: MediaClient,
    pub comments: CommentsClient,
    pub feedback: FeedbackClient,
    pub annotation_queues: AnnotationQueuesClient,
    pub blob_storage_integrations: BlobStorageIntegrationsClient,
    pub evaluation_rules: EvaluationRulesClient,
    pub evaluators: EvaluatorsClient,
    pub experiments: ExperimentsClient,
    pub llm_connections: LlmConnectionsClient,
    pub metrics: MetricsClient,
    pub organizations: OrganizationsClient,
    pub scim: ScimClient,
    #[cfg(feature = "unstable")]
    pub unstable_dashboard_widgets: UnstableDashboardWidgetsClient,
    #[cfg(feature = "unstable")]
    pub unstable_dashboards: UnstableDashboardsClient,
    #[cfg(feature = "unstable")]
    pub unstable_topics: UnstableTopicsClient,
}

impl LangfuseClient {
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
