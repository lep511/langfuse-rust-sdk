use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::skills::{
    CreateSkillVersionRequest, DeleteSkillVersionResponse, SkillFileContentsResponse,
    SkillListResponse, SkillVersion,
};

pub use crate::models::skills::SkillMeta;

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

/// Select a skill version by `version` or `label` (default: the `production` label).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSkillParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Include UTF-8 file content inline (server default `false`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_content: Option<bool>,
}

/// Body of `PATCH /api/public/skills/{skillName}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillRequest {
    /// Complete set of tags for every version of the skill. Empty clears all tags.
    pub tags: Vec<String>,
}

/// Body of `PATCH /api/public/skills/{skillName}/versions/{skillVersion}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSkillVersionLabelsRequest {
    /// Labels to set on the version; they are moved from other versions atomically.
    pub labels: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileContentsQuery {
    sha256_hashes: String,
}

impl SkillsClient {
    pub async fn list(
        &self,
        params: &ListSkillsParams,
    ) -> Result<SkillListResponse, LangfuseError> {
        self.http.get_with_query("/api/public/skills", params).await
    }

    /// `POST /api/public/skills` (201).
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
        self.http.get_with_query(&skill_path(name), params).await
    }

    /// Replace the shared tags of every version. Returns the latest version.
    pub async fn update(
        &self,
        name: &str,
        req: &UpdateSkillRequest,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http.patch(&skill_path(name), req).await
    }

    /// `GET /api/public/skills/files/content`: read up to 50 file contents by hash.
    pub async fn get_file_contents(
        &self,
        sha256_hashes: &[&str],
    ) -> Result<SkillFileContentsResponse, LangfuseError> {
        let query = FileContentsQuery {
            sha256_hashes: sha256_hashes.join(","),
        };
        self.http
            .get_with_query("/api/public/skills/files/content", &query)
            .await
    }

    pub async fn set_version_labels(
        &self,
        name: &str,
        version: i64,
        req: &SetSkillVersionLabelsRequest,
    ) -> Result<SkillVersion, LangfuseError> {
        self.http.patch(&version_path(name, version), req).await
    }

    pub async fn delete_version(
        &self,
        name: &str,
        version: i64,
    ) -> Result<DeleteSkillVersionResponse, LangfuseError> {
        self.http
            .delete_with_response(&version_path(name, version))
            .await
    }
}

fn skill_path(name: &str) -> String {
    format!("/api/public/skills/{}", encode_path_segment(name))
}

fn version_path(name: &str, version: i64) -> String {
    format!("{}/versions/{version}", skill_path(name))
}
