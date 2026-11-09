use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::prompts::{CreatePromptRequest, Prompt};
use crate::pagination::Paginated;

pub struct PromptsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_updated_at: Option<String>,
}

/// Metadata returned for each prompt in the list endpoint (no prompt content).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMeta {
    pub name: String,
    pub version: Option<i64>,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub r#type: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl PromptsClient {
    pub async fn list(
        &self,
        params: &ListPromptsParams,
    ) -> Result<Paginated<PromptMeta>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v2/prompts", params)
            .await
    }

    pub async fn create(&self, req: &CreatePromptRequest) -> Result<Prompt, LangfuseError> {
        self.http.post("/api/public/v2/prompts", req).await
    }

    pub async fn get(&self, name: &str) -> Result<Prompt, LangfuseError> {
        self.http
            .get(&format!("/api/public/v2/prompts/{name}"))
            .await
    }

    pub async fn get_with_params(
        &self,
        name: &str,
        params: &GetPromptParams,
    ) -> Result<Prompt, LangfuseError> {
        self.http
            .get_with_query(&format!("/api/public/v2/prompts/{name}"), params)
            .await
    }

    pub async fn delete(&self, name: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/v2/prompts/{name}"))
            .await
    }
}
