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
    pub sha256_hash: String,
    pub content_type: String,
    pub content_length: i64,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillVersionRequest {
    pub files: Vec<SkillVersionFileInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionFileInput {
    pub path: String,
    pub content: String,
}
