use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{ScoreDataType, ScoreSource};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseScore {
    pub id: String,
    pub name: String,
    pub source: ScoreSource,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub environment: String,
    pub trace_id: Option<String>,
    pub session_id: Option<String>,
    pub observation_id: Option<String>,
    pub dataset_run_id: Option<String>,
    pub author_user_id: Option<String>,
    pub comment: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub config_id: Option<String>,
    pub queue_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "dataType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScoreV3 {
    #[serde(rename_all = "camelCase")]
    Numeric {
        #[serde(flatten)]
        base: BaseScore,
        value: f64,
    },
    #[serde(rename_all = "camelCase")]
    Boolean {
        #[serde(flatten)]
        base: BaseScore,
        value: f64,
        string_value: String,
    },
    #[serde(rename_all = "camelCase")]
    Categorical {
        #[serde(flatten)]
        base: BaseScore,
        value: Option<f64>,
        string_value: String,
    },
    #[serde(rename_all = "camelCase")]
    Correction {
        #[serde(flatten)]
        base: BaseScore,
        value: Option<f64>,
        string_value: String,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        #[serde(flatten)]
        base: BaseScore,
        string_value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CreateScoreValue {
    Numeric(f64),
    String(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreRequest {
    pub name: String,
    pub value: CreateScoreValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<ScoreDataType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreResponse {
    pub id: String,
}
