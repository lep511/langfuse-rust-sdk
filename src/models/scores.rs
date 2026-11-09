use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{ScoreDataType, ScoreSource};

/// Fields shared by every score variant (spec `BaseScoreV3`).
///
/// Some fields are only present when the matching field group is requested
/// via `ListScoresParams::fields` (`details`, `annotation`, `subject`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseScore {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub source: ScoreSource,
    pub timestamp: DateTime<Utc>,
    pub environment: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub comment: Option<String>,
    pub config_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub author_user_id: Option<String>,
    pub queue_id: Option<String>,
    /// The entity the score is attached to. Present when `fields` includes `subject`.
    pub subject: Option<ScoreSubjectV3>,
}

/// The entity a score is attached to (spec `ScoreSubjectV3`, discriminated by `kind`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ScoreSubjectV3 {
    Trace {
        id: String,
    },
    #[serde(rename_all = "camelCase")]
    Observation {
        id: String,
        trace_id: Option<String>,
    },
    Session {
        id: String,
    },
    /// `id` is the dataset run (experiment) id.
    Experiment {
        id: String,
    },
}

/// A score from `GET /api/public/v3/scores` (spec `ScoreV3`, discriminated by `dataType`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "dataType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScoreV3 {
    Numeric {
        #[serde(flatten)]
        base: BaseScore,
        value: f64,
    },
    Boolean {
        #[serde(flatten)]
        base: BaseScore,
        value: bool,
    },
    Categorical {
        #[serde(flatten)]
        base: BaseScore,
        value: String,
    },
    Text {
        #[serde(flatten)]
        base: BaseScore,
        value: String,
    },
    /// `value` is an empty string when no correction is set.
    Correction {
        #[serde(flatten)]
        base: BaseScore,
        value: String,
    },
}

impl ScoreV3 {
    /// Fields shared by every variant.
    pub fn base(&self) -> &BaseScore {
        match self {
            ScoreV3::Numeric { base, .. }
            | ScoreV3::Boolean { base, .. }
            | ScoreV3::Categorical { base, .. }
            | ScoreV3::Text { base, .. }
            | ScoreV3::Correction { base, .. } => base,
        }
    }
}

/// Score value: a number for numeric/boolean scores (booleans are `1` or `0`),
/// a string for categorical/text scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CreateScoreValue {
    Numeric(f64),
    String(String),
}

/// Source accepted when creating a score. `EVAL` is reserved for internal evaluators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CreateScoreSource {
    Api,
    Annotation,
}

/// One item of the `POST /api/public/scores` batch body (spec `CreateScoreRequest`).
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
    /// Defaults to `API` server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CreateScoreSource>,
    /// Defaults to the time the request is received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl CreateScoreRequest {
    /// A request with only the required fields set.
    pub fn new(name: impl Into<String>, value: CreateScoreValue) -> Self {
        Self {
            name: name.into(),
            value,
            id: None,
            trace_id: None,
            session_id: None,
            observation_id: None,
            dataset_run_id: None,
            comment: None,
            metadata: None,
            environment: None,
            queue_id: None,
            data_type: None,
            config_id: None,
            source: None,
            timestamp: None,
        }
    }
}

/// Response of `POST /api/public/scores` (spec `CreateScoresResponse`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CreateScoresResponse {
    /// HTTP 207: some scores were rejected. Do not retry the whole batch automatically.
    BatchResults(CreateScoreBatchResults),
    /// HTTP 202: every score was accepted.
    Accepted(CreateScoreBatchResponse),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreBatchResponse {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreBatchResults {
    pub accepted: i64,
    pub rejected: i64,
    pub errors: Vec<CreateScoreBatchError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScoreBatchError {
    pub message: String,
}
