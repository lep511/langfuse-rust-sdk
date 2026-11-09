//! Experiment response types.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::enums::ObservationLevel;
use super::scores::ScoreV3;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Experiment {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub item_count: i64,
    pub dataset_id: Option<String>,
    /// Present when `fields` includes `metadata`.
    pub metadata: Option<serde_json::Value>,
    /// Present when `fields` includes `scores`.
    pub scores: Option<Vec<ScoreV3>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentItem {
    pub id: String,
    pub trace_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub level: ObservationLevel,
    pub environment: String,
    pub experiment_id: String,
    pub experiment_name: String,
    pub experiment_item_id: String,
    pub experiment_dataset_id: Option<String>,
    pub experiment_item_version: Option<DateTime<Utc>>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub expected_output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub experiment_item_metadata: Option<serde_json::Value>,
    pub experiment_metadata: Option<serde_json::Value>,
    pub experiment_description: Option<String>,
    pub scores: Option<Vec<ScoreV3>>,
}
