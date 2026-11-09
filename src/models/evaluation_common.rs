//! Types shared by the evaluators and evaluation-rules APIs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::LangfuseError;

/// User who created a resource.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    pub id: String,
    pub name: Option<String>,
}

/// Data that populates an evaluator prompt variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptVariableMappingSource {
    Input,
    Output,
    Metadata,
    /// Tool calls of the observation, as `{id, name, arguments, type, index}` objects.
    ToolCalls,
    /// Expected output of the experiment item.
    ExpectedOutput,
    ExperimentItemMetadata,
}

/// Connects one prompt variable to observation or experiment data (request side).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptVariableMappingInput {
    /// Variable name without braces, as listed in the evaluator `variables`.
    pub variable: String,
    pub source: PromptVariableMappingSource,
    /// JSONPath (starting with `$`) applied to the source before insertion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_path: Option<String>,
}

/// A stored prompt variable mapping (response side, spec `PromptVariableMapping`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PromptVariableMapping {
    /// Deprecated mapping of a legacy trace or dataset evaluation rule.
    Legacy(LegacyPromptVariableMapping),
    Read(PromptVariableMappingRead),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptVariableMappingRead {
    pub variable: String,
    /// `None` when the mapping is incomplete.
    pub source: Option<String>,
    pub json_path: Option<String>,
}

/// Deprecated mapping (`"mappingType": "legacy"`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPromptVariableMapping {
    pub mapping_type: String,
    pub variable: String,
    pub langfuse_object: LegacyEvaluationObject,
    /// Observation name, `None` for trace and dataset-item mappings.
    pub object_name: Option<String>,
    pub source: String,
    pub json_path: Option<String>,
}

wire_enum!(
    /// Deprecated object kind used by legacy evaluation rules.
    LegacyEvaluationObject, "snake_case", {
    Trace,
    Span,
    Generation,
    Event,
    Agent,
    Tool,
    Chain,
    Retriever,
    Evaluator,
    Embedding,
    Guardrail,
    DatasetItem,
});

/// Stored filter returned verbatim by evaluation-rule read endpoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRuleReadFilter {
    pub r#type: String,
    pub column: String,
    pub operator: String,
    /// Shape depends on `type`.
    pub value: Option<serde_json::Value>,
    /// Present for object filters (e.g. `metadata`).
    pub key: Option<String>,
}

wire_enum!(
    /// Machine-readable error code of [`PublicApiError`].
    PublicApiErrorCode, "snake_case", {
    AuthenticationFailed,
    AccessDenied,
    InvalidRequest,
    InvalidQuery,
    InvalidBody,
    ResourceNotFound,
    Conflict,
    RateLimited,
    MethodNotAllowed,
    InternalError,
});

/// Error body of the evaluators and evaluation-rules endpoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiError {
    pub message: String,
    pub code: PublicApiErrorCode,
    pub details: Option<PublicApiErrorDetails>,
}

impl PublicApiError {
    /// Parse the JSON body of an API error, if it has this shape.
    pub fn from_error(error: &LangfuseError) -> Option<Self> {
        serde_json::from_value(error.body()?.clone()).ok()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiErrorDetails {
    pub issues: Option<Vec<PublicApiValidationIssue>>,
    pub retry_after_seconds: Option<i64>,
    pub limit: Option<i64>,
    pub remaining: Option<i64>,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiValidationIssue {
    pub code: String,
    pub message: String,
    /// Path to the invalid field; items are strings or integers.
    pub path: Vec<serde_json::Value>,
}
