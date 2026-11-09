use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::llm_connections::{
    DeleteLlmConnectionResponse, LlmConnection, UpsertLlmConnectionRequest,
};
use crate::pagination::{PageParams, Paginated};

const PROJECT_BASE: &str = "/api/public/llm-connections";
const ORGANIZATION_BASE: &str = "/api/public/organizations/llm-connections";

/// LLM connections at project level and, with an organization-scoped API key,
/// at organization level (inherited by projects without an override).
pub struct LlmConnectionsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl LlmConnectionsClient {
    /// `GET /api/public/llm-connections`
    pub async fn list(
        &self,
        params: &PageParams,
    ) -> Result<Paginated<LlmConnection>, LangfuseError> {
        self.http.get_with_query(PROJECT_BASE, params).await
    }

    /// `PUT /api/public/llm-connections` (upsert on `provider`).
    pub async fn upsert(
        &self,
        req: &UpsertLlmConnectionRequest,
    ) -> Result<LlmConnection, LangfuseError> {
        self.http.put(PROJECT_BASE, req).await
    }

    /// `DELETE /api/public/llm-connections/{id}`. Dependent evaluators are paused.
    pub async fn delete(&self, id: &str) -> Result<DeleteLlmConnectionResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!("{PROJECT_BASE}/{}", encode_path_segment(id)))
            .await
    }

    /// `GET /api/public/organizations/llm-connections`
    pub async fn list_organization(
        &self,
        params: &PageParams,
    ) -> Result<Paginated<LlmConnection>, LangfuseError> {
        self.http.get_with_query(ORGANIZATION_BASE, params).await
    }

    /// `PUT /api/public/organizations/llm-connections` (upsert on `provider`).
    pub async fn upsert_organization(
        &self,
        req: &UpsertLlmConnectionRequest,
    ) -> Result<LlmConnection, LangfuseError> {
        self.http.put(ORGANIZATION_BASE, req).await
    }

    /// `DELETE /api/public/organizations/llm-connections/{id}`
    pub async fn delete_organization(
        &self,
        id: &str,
    ) -> Result<DeleteLlmConnectionResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!("{ORGANIZATION_BASE}/{}", encode_path_segment(id)))
            .await
    }
}
