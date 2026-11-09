use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::projects::Project;

pub struct ProjectsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsListResponse {
    pub data: Vec<Project>,
}

impl ProjectsClient {
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
        self.http
            .put(&format!("/api/public/projects/{project_id}"), req)
            .await
    }

    pub async fn delete(&self, project_id: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/projects/{project_id}"))
            .await
    }
}
