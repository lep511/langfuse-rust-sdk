use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::DatasetStatus;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetItem {
    pub id: String,
    pub status: DatasetStatus,
    pub input: Option<serde_json::Value>,
    pub expected_output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub source_trace_id: Option<String>,
    pub source_observation_id: Option<String>,
    pub dataset_id: String,
    pub dataset_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When this version of the item became current.
    pub version: Option<DateTime<Utc>>,
    /// Langfuse media references found in `input`, `expected_output` and `metadata`.
    #[serde(default)]
    pub media_references: Vec<DatasetItemMediaReference>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetItemMediaReference {
    pub field: DatasetItemMediaReferenceField,
    /// e.g. `@@@langfuseMedia:type=image/png|id=...|source=bytes@@@`
    pub reference_string: String,
    /// JSONPath of the string holding the reference within the field, e.g. `$['image']`.
    pub json_path: String,
    pub media: DatasetItemMediaReferenceMedia,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DatasetItemMediaReferenceField {
    Input,
    ExpectedOutput,
    Metadata,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetItemMediaReferenceMedia {
    pub media_id: String,
    pub content_type: String,
    pub content_length: i64,
    /// Signed download URL.
    pub url: String,
    pub url_expiry: String,
}

/// Response of `DELETE /api/public/dataset-items/{id}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDatasetItemResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatasetItemRequest {
    pub dataset_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_observation_id: Option<String>,
    /// Items are upserted on `id` (unique per project, at most 255 characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<DatasetStatus>,
}
