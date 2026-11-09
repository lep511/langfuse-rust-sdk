use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::projects::{
    ApiKeyDeletionResponse, ApiKeyList, ApiKeyResponse, Project, ProjectDeletionResponse,
};

pub struct ProjectsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Body of `POST /api/public/projects` (organization-scoped API key).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Days to retain data: `0` (no limit) or at least `3`.
    pub retention: i64,
}

/// Body of `PUT /api/public/projects/{projectId}` (organization-scoped API key).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Days to retain data. `None` keeps the current setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<i64>,
}

/// Body of `POST /api/public/projects/{projectId}/apiKeys`. Every field is optional.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Deprecated alias of `name`; cannot be sent together with `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Must be in the future.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Custom public key (`pk-lf-...`); must be sent together with `secret_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Custom secret key (`sk-lf-...`); must be sent together with `public_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsListResponse {
    pub data: Vec<Project>,
}

impl ProjectsClient {
    /// `GET /api/public/projects`: the project of a project-scoped API key.
    pub async fn list(&self) -> Result<ProjectsListResponse, LangfuseError> {
        self.http.get("/api/public/projects").await
    }

    pub async fn create(&self, req: &CreateProjectRequest) -> Result<Project, LangfuseError> {
        self.http.post("/api/public/projects", req).await
    }

    pub async fn update(
        &self,
        project_id: &str,
        req: &UpdateProjectRequest,
    ) -> Result<Project, LangfuseError> {
        self.http.put(&project_path(project_id), req).await
    }

    /// `DELETE /api/public/projects/{projectId}`. Deletion is processed asynchronously (202).
    pub async fn delete(&self, project_id: &str) -> Result<ProjectDeletionResponse, LangfuseError> {
        self.http
            .delete_with_response(&project_path(project_id))
            .await
    }

    /// `GET /api/public/projects/{projectId}/apiKeys`
    pub async fn get_api_keys(&self, project_id: &str) -> Result<ApiKeyList, LangfuseError> {
        self.http
            .get(&format!("{}/apiKeys", project_path(project_id)))
            .await
    }

    /// `POST /api/public/projects/{projectId}/apiKeys`. The response holds the full secret key.
    pub async fn create_api_key(
        &self,
        project_id: &str,
        req: &CreateApiKeyRequest,
    ) -> Result<ApiKeyResponse, LangfuseError> {
        self.http
            .post(&format!("{}/apiKeys", project_path(project_id)), req)
            .await
    }

    /// `DELETE /api/public/projects/{projectId}/apiKeys/{apiKeyId}`
    pub async fn delete_api_key(
        &self,
        project_id: &str,
        api_key_id: &str,
    ) -> Result<ApiKeyDeletionResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!(
                "{}/apiKeys/{}",
                project_path(project_id),
                encode_path_segment(api_key_id)
            ))
            .await
    }
}

fn project_path(project_id: &str) -> String {
    format!("/api/public/projects/{}", encode_path_segment(project_id))
}
