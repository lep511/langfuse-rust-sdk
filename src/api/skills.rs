use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::skills::{CreateSkillVersionRequest, SkillVersion};
use crate::pagination::Paginated;

pub struct SkillsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSkillsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSkillParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_content: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSkillVersionLabelsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

/// Metadata for a skill in list responses (no file content).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl SkillsClient {
    pub async fn list(
        &self,
        params: &ListSkillsParams,
    ) -> Result<Paginated<SkillMeta>, LangfuseError> {
        self.http
            .get_with_query("/api/public/skills", params)
            .await
    }

    pub async fn create_version(
        &self,
        req: &CreateSkillVersionRequest,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http.post("/api/public/skills", req).await
    }

    pub async fn get(
        &self,
        name: &str,
        params: &GetSkillParams,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http
            .get_with_query(&format!("/api/public/skills/{name}"), params)
            .await
    }

    pub async fn update(
        &self,
        name: &str,
        req: &UpdateSkillRequest,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http
            .patch(&format!("/api/public/skills/{name}"), req)
            .await
    }

    pub async fn set_version_labels(
        &self,
        name: &str,
        version: i64,
        req: &SetSkillVersionLabelsRequest,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http
            .patch(
                &format!("/api/public/skills/{name}/versions/{version}"),
                req,
            )
            .await
    }

    pub async fn delete_version(
        &self,
        name: &str,
        version: i64,
    ) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/skills/{name}/versions/{version}"))
            .await
    }
}
