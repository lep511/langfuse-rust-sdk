//! Annotation queue request and response types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum AnnotationQueueStatus {
    Pending,
    Completed,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum AnnotationQueueObjectType {
    Trace,
    Observation,
    Session,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationQueue {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub score_config_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationQueueItem {
    pub id: String,
    pub queue_id: String,
    pub object_id: String,
    pub object_type: AnnotationQueueObjectType,
    pub status: AnnotationQueueStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAnnotationQueueRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub score_config_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAnnotationQueueItemRequest {
    pub object_id: String,
    pub object_type: AnnotationQueueObjectType,
    /// Server default `PENDING`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<AnnotationQueueStatus>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAnnotationQueueItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<AnnotationQueueStatus>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAnnotationQueueItemResponse {
    pub success: bool,
    pub message: String,
}

/// Body of the assignment endpoints (`POST`/`DELETE .../assignments`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationQueueAssignmentRequest {
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAnnotationQueueAssignmentResponse {
    pub user_id: String,
    pub queue_id: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAnnotationQueueAssignmentResponse {
    pub success: bool,
}
