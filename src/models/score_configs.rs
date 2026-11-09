//! Score configuration request and response types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::ScoreConfigDataType;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigCategory {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreConfig {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_id: String,
    pub data_type: ScoreConfigDataType,
    pub is_archived: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub categories: Option<Vec<ConfigCategory>>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreConfigRequest {
    pub name: String,
    pub data_type: ScoreConfigDataType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<CreateConfigCategory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigCategory {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScoreConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<CreateConfigCategory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
