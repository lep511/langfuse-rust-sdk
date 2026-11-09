//! Types shared by the unstable APIs (feature `unstable`). These APIs may change
//! without notice upstream.

use serde::{Deserialize, Serialize};

use super::evaluation_common::{PublicApiErrorCode, PublicApiValidationIssue};
use crate::error::LangfuseError;

/// Error body of the unstable endpoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnstablePublicApiError {
    pub message: String,
    pub code: PublicApiErrorCode,
    pub details: Option<UnstablePublicApiErrorDetails>,
}

impl UnstablePublicApiError {
    /// Parse the JSON body of an API error, if it has this shape.
    pub fn from_error(error: &LangfuseError) -> Option<Self> {
        serde_json::from_value(error.body()?.clone()).ok()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnstablePublicApiErrorDetails {
    pub issues: Option<Vec<PublicApiValidationIssue>>,
    pub field: Option<String>,
    pub column: Option<String>,
    pub invalid_values: Option<Vec<String>>,
    pub allowed_values: Option<Vec<String>>,
    pub value: Option<String>,
    pub retry_after_seconds: Option<i64>,
    pub limit: Option<i64>,
    pub remaining: Option<i64>,
    /// Unformatted string in the spec.
    pub reset_at: Option<String>,
}

/// A filter in Langfuse filter-state shape, used by widgets, dashboards and the
/// `filter` query parameter of the Topics endpoints (JSON-encoded).
///
/// `type` decides the operators and the `value` shape: `string`, `number`,
/// `datetime`, `boolean`, `null` (value `""`), `stringOptions`, `arrayOptions`,
/// `categoryOptions`, `stringObject`, `numberObject`, `booleanObject`
/// (the last four and `categoryOptions` require `key`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetFilter {
    pub column: String,
    pub operator: String,
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}
