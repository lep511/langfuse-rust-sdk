//! Skill request and response types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersion {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_id: String,
    pub created_by: String,
    pub name: String,
    pub description: Option<String>,
    pub frontmatter: Option<serde_json::Value>,
    pub version: i64,
    pub tags: Vec<String>,
    pub labels: Vec<String>,
    pub commit_message: Option<String>,
    pub files: Vec<SkillFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub id: String,
    pub path: String,
    pub blob_id: String,
    /// Canonical base64-encoded SHA-256; use with `SkillsClient::get_file_contents`.
    pub sha256_hash: String,
    pub content_type: String,
    pub content_length: i64,
    /// UTF-8 content, present when fetched with `include_content`.
    pub content: Option<String>,
}

/// Metadata for a skill in list responses (no file content).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub latest_version: i64,
    pub latest_version_created_at: DateTime<Utc>,
    pub latest_version_last_updated_at: DateTime<Utc>,
    /// Version with the `production` label, if any.
    pub production_version: Option<i64>,
}

/// Page metadata of `GET /api/public/skills` (page-based meta plus `hasNextPage`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListPagination {
    pub page: i64,
    pub limit: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListResponse {
    pub data: Vec<SkillMeta>,
    pub meta: SkillListPagination,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillVersionRequest {
    pub files: Vec<SkillVersionFileCreateInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
}

/// One file of a new skill version: either inline content or a reference to
/// content that already exists in the project.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SkillVersionFileCreateInput {
    Content(SkillVersionFileInput),
    Reference(SkillVersionFileReference),
}

impl SkillVersionFileCreateInput {
    /// A file with inline UTF-8 content.
    pub fn content(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Content(SkillVersionFileInput {
            path: path.into(),
            content: content.into(),
        })
    }

    /// A file referencing existing content by its canonical base64 SHA-256 hash.
    pub fn reference(path: impl Into<String>, sha256_hash: impl Into<String>) -> Self {
        Self::Reference(SkillVersionFileReference {
            path: path.into(),
            sha256_hash: sha256_hash.into(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionFileInput {
    pub path: String,
    /// UTF-8 text content. The server computes the hash and byte length.
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionFileReference {
    pub path: String,
    pub sha256_hash: String,
}

/// Response of `GET /api/public/skills/files/content`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileContentsResponse {
    pub data: Vec<SkillFileContent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileContent {
    pub sha256_hash: String,
    pub content: String,
}

/// Response of `DELETE /api/public/skills/{skillName}/versions/{skillVersion}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillVersionResponse {
    pub deleted: bool,
}
