//! Ingestion request types for batch event submission.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestTracesRequest {
    pub resource_spans: Vec<OtelResourceSpan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelResourceSpan {
    pub resource: Option<OtelResource>,
    pub scope_spans: Vec<OtelScopeSpan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelResource {
    pub attributes: Option<Vec<OtelKeyValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelScopeSpan {
    pub scope: Option<OtelInstrumentationScope>,
    pub spans: Vec<OtelSpan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelInstrumentationScope {
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelKeyValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    /// OTLP span kind (0 unspecified, 1 internal, 2 server, 3 client, 4 producer, 5 consumer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<i64>,
    pub start_time_unix_nano: String,
    pub end_time_unix_nano: String,
    pub attributes: Option<Vec<OtelKeyValue>>,
    pub status: Option<OtelStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelKeyValue {
    pub key: String,
    pub value: OtelAnyValue,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelAnyValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelStatus {
    pub code: Option<i32>,
    pub message: Option<String>,
}
