//! Retrieve, create, update and delete prompts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::enums::PromptType;
use crate::models::prompts::{CreatePromptRequest, Prompt};
use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};

#[derive(Clone)]
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
    /// JSON-encoded array of filter conditions. When set, it takes precedence over
    /// `name`, `label`, `tag`, `from_updated_at` and `to_updated_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl_page_query!(ListPromptsParams);

/// Metadata returned for each prompt in the list endpoint (no prompt content).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMeta {
    pub name: String,
    pub r#type: PromptType,
    pub versions: Vec<i64>,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub last_updated_at: DateTime<Utc>,
    /// Config of the most recent matching version (`null` when unset).
    #[serde(default)]
    pub last_config: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Resolve prompt dependencies (server default `true`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve: Option<bool>,
}

/// Restrict a prompt deletion to one label or version. With neither set, every
/// version of the prompt is deleted.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePromptParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
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

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: ListPromptsParams) -> Pages<'_, Paginated<PromptMeta>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    pub async fn create(&self, req: &CreatePromptRequest) -> Result<Prompt, LangfuseError> {
        self.http.post("/api/public/v2/prompts", req).await
    }

    pub async fn get(&self, name: &str) -> Result<Prompt, LangfuseError> {
        self.http.get(&prompt_path(name)).await
    }

    pub async fn get_with_params(
        &self,
        name: &str,
        params: &GetPromptParams,
    ) -> Result<Prompt, LangfuseError> {
        self.http.get_with_query(&prompt_path(name), params).await
    }

    /// `DELETE /api/public/v2/prompts/{promptName}`: delete every version (204).
    pub async fn delete(&self, name: &str) -> Result<(), LangfuseError> {
        self.http.delete_no_content(&prompt_path(name)).await
    }

    /// `DELETE /api/public/v2/prompts/{promptName}?label=..&version=..` (204).
    pub async fn delete_with_params(
        &self,
        name: &str,
        params: &DeletePromptParams,
    ) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content_with_query(&prompt_path(name), params)
            .await
    }
}

fn prompt_path(name: &str) -> String {
    format!("/api/public/v2/prompts/{}", encode_path_segment(name))
}
