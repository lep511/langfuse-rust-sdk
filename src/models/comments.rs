use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Object a comment is attached to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum CommentObjectType {
    Trace,
    Observation,
    Session,
    Prompt,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub project_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub object_type: CommentObjectType,
    pub object_id: String,
    pub content: String,
    pub author_user_id: Option<String>,
}

/// Body of `POST /api/public/comments`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    pub project_id: String,
    /// `trace`, `observation`, `session` or `prompt`.
    pub object_type: String,
    /// Must reference an existing object.
    pub object_id: String,
    /// Markdown allowed, at most 5000 characters.
    pub content: String,
    /// Must be a member of the organization that owns the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
}

impl CreateCommentRequest {
    pub fn new(
        project_id: impl Into<String>,
        object_type: impl Into<String>,
        object_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            object_type: object_type.into(),
            object_id: object_id.into(),
            content: content.into(),
            author_user_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentResponse {
    pub id: String,
}
