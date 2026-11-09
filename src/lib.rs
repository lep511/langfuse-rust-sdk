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

use crate::api::dataset_items::DatasetItemsClient;
use crate::api::datasets::DatasetsClient;
use crate::api::health::HealthClient;
use crate::api::ingestion::IngestionClient;
use crate::api::media::MediaClient;
use crate::api::models::ModelsClient;
use crate::api::observations::ObservationsClient;
use crate::api::projects::ProjectsClient;
use crate::api::prompt_version::PromptVersionClient;
use crate::api::prompts::PromptsClient;
use crate::api::score_configs::ScoreConfigsClient;
use crate::api::scores::ScoresClient;
use crate::api::skills::SkillsClient;
use crate::api::traces::TracesClient;
use crate::http::HttpClient;

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
}

impl LangfuseClient {
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let http = Arc::new(HttpClient::new(config)?);
        Ok(Self {
            health: HealthClient { http: Arc::clone(&http) },
            scores: ScoresClient { http: Arc::clone(&http) },
            score_configs: ScoreConfigsClient { http: Arc::clone(&http) },
            observations: ObservationsClient { http: Arc::clone(&http) },
            datasets: DatasetsClient { http: Arc::clone(&http) },
            dataset_items: DatasetItemsClient { http: Arc::clone(&http) },
            prompts: PromptsClient { http: Arc::clone(&http) },
            prompt_versions: PromptVersionClient { http: Arc::clone(&http) },
            models: ModelsClient { http: Arc::clone(&http) },
            skills: SkillsClient { http: Arc::clone(&http) },
            projects: ProjectsClient { http: Arc::clone(&http) },
            traces: TracesClient { http: Arc::clone(&http) },
            ingestion: IngestionClient { http: Arc::clone(&http) },
            media: MediaClient { http },
        })
    }
}
