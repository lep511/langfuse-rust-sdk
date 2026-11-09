use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::scores::{CreateScoreRequest, CreateScoresResponse, ScoreV3};
use crate::pagination::CursorPaginated;

pub struct ScoresClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListScoresParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
}

impl ScoresClient {
    /// `GET /api/public/v3/scores` (cursor-paginated).
    pub async fn list(
        &self,
        params: &ListScoresParams,
    ) -> Result<CursorPaginated<ScoreV3>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v3/scores", params)
            .await
    }

    /// `POST /api/public/scores`: create a batch of scores asynchronously.
    ///
    /// Returns [`CreateScoresResponse::Accepted`] (HTTP 202) when every score was
    /// accepted, or [`CreateScoresResponse::BatchResults`] (HTTP 207) with per-score
    /// errors. Do not retry a 207 batch automatically.
    pub async fn create(
        &self,
        scores: &[CreateScoreRequest],
    ) -> Result<CreateScoresResponse, LangfuseError> {
        self.http.post("/api/public/scores", &scores).await
    }

    /// `DELETE /api/public/scores/{scoreId}` (204).
    pub async fn delete(&self, score_id: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!(
                "/api/public/scores/{}",
                encode_path_segment(score_id)
            ))
            .await
    }
}
