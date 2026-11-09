use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::scores::{CreateScoreRequest, CreateScoreResponse, ScoreV3};
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
}

impl ScoresClient {
    pub async fn list(
        &self,
        params: &ListScoresParams,
    ) -> Result<CursorPaginated<ScoreV3>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v3/scores", params)
            .await
    }

    pub async fn create(
        &self,
        req: &CreateScoreRequest,
    ) -> Result<CreateScoreResponse, LangfuseError> {
        self.http.post("/api/public/scores", req).await
    }

    pub async fn delete(&self, score_id: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/scores/{score_id}"))
            .await
    }
}
