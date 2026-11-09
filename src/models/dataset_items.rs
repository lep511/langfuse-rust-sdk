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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<DatasetStatus>,
}
