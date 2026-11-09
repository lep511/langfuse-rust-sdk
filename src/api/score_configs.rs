//! Manage score configuration presets.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::score_configs::{
    CreateScoreConfigRequest, ScoreConfig, UpdateScoreConfigRequest,
};
use crate::pagination::{PageParams, Paginated};

pub struct ScoreConfigsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl ScoreConfigsClient {
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<ScoreConfig>, LangfuseError> {
        self.http
            .get_with_query("/api/public/score-configs", params)
            .await
    }

    pub async fn create(
        &self,
        req: &CreateScoreConfigRequest,
    ) -> Result<ScoreConfig, LangfuseError> {
        self.http.post("/api/public/score-configs", req).await
    }

    pub async fn get(&self, config_id: &str) -> Result<ScoreConfig, LangfuseError> {
        self.http.get(&config_path(config_id)).await
    }

    pub async fn update(
        &self,
        config_id: &str,
        req: &UpdateScoreConfigRequest,
    ) -> Result<ScoreConfig, LangfuseError> {
        self.http.patch(&config_path(config_id), req).await
    }
}

fn config_path(config_id: &str) -> String {
    format!(
        "/api/public/score-configs/{}",
        encode_path_segment(config_id)
    )
}
