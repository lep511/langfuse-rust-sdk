//! Project response types.

use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub organization: Organization,
    pub metadata: Option<serde_json::Value>,
    /// Days to retain data. `None` or `0` means no retention limit.
    pub retention_days: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub name: String,
}

/// Response of `DELETE /api/public/projects/{projectId}` (HTTP 202).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyList {
    pub api_keys: Vec<ApiKeySummary>,
}

/// An API key without its secret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeySummary {
    pub id: String,
    pub created_at: DateTime<Utc>,
    /// `None` if the key does not expire.
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub name: Option<String>,
    /// Deprecated alias of `name`.
    pub note: Option<String>,
    pub public_key: String,
    pub display_secret_key: String,
}

/// A newly created API key, including its secret.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyResponse {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub public_key: String,
    pub secret_key: String,
    pub display_secret_key: String,
    pub name: Option<String>,
    /// Deprecated alias of `name`.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyDeletionResponse {
    pub success: bool,
}
