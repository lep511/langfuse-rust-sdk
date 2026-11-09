//! Dataset and dataset-run request and response types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::dataset_items::DatasetItem;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dataset {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub input_schema: Option<serde_json::Value>,
    pub expected_output_schema: Option<serde_json::Value>,
    pub project_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatasetRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output_schema: Option<serde_json::Value>,
}

/// A dataset with all its items, from
/// [`LangfuseClient::get_dataset`](crate::LangfuseClient::get_dataset).
///
/// Media in the items stays as reference strings
/// (`@@@langfuseMedia:...@@@`); each item's `media_references` carries signed
/// download URLs, and [`MediaClient::resolve_references`](crate::api::media::MediaClient::resolve_references)
/// turns the references into data URIs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DatasetWithItems {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub project_id: String,
    pub metadata: Option<serde_json::Value>,
    pub input_schema: Option<serde_json::Value>,
    pub expected_output_schema: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Every item of the dataset, in API order.
    pub items: Vec<DatasetItem>,
    /// The version the items were read at (`None`: the latest).
    pub version: Option<DateTime<Utc>>,
}

impl DatasetWithItems {
    pub(crate) fn new(
        dataset: Dataset,
        items: Vec<DatasetItem>,
        version: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: dataset.id,
            name: dataset.name,
            description: dataset.description,
            project_id: dataset.project_id,
            metadata: dataset.metadata,
            input_schema: dataset.input_schema,
            expected_output_schema: dataset.expected_output_schema,
            created_at: dataset.created_at,
            updated_at: dataset.updated_at,
            items,
            version,
        }
    }
}
