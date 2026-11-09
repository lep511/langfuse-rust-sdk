//! Ingestion request types for OTLP/HTTP JSON trace export.
//!
//! The constructors keep attribute lists short:
//!
//! ```
//! use langfuse_rust_sdk::models::ingestion::{OtelKeyValue, OtelSpan};
//!
//! let span = OtelSpan {
//!     attributes: Some(vec![
//!         OtelKeyValue::string("langfuse.observation.type", "span"),
//!         OtelKeyValue::strings("langfuse.trace.tags", ["prod", "checkout"]),
//!         OtelKeyValue::json("langfuse.observation.input", &serde_json::json!({"q": "hi"}))
//!             .unwrap(),
//!     ]),
//!     ..OtelSpan::new(
//!         "4bf92f3577b34da6a3ce929d0e0e4736",
//!         "00f067aa0ba902b7",
//!         "handle-request",
//!         "1718445600000000000",
//!         "1718445601000000000",
//!     )
//! };
//! # let _ = span;
//! ```
//!
//! Verified against the Langfuse API: `arrayValue` is decoded (string lists such
//! as `langfuse.trace.tags` become JSON arrays). Nested objects should be sent as
//! JSON in a `stringValue` ([`OtelAnyValue::json`]).

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
    /// W3C `tracestate` of the span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_state: Option<String>,
    pub name: String,
    /// OTLP span kind (0 unspecified, 1 internal, 2 server, 3 client, 4 producer, 5 consumer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<i64>,
    pub start_time_unix_nano: String,
    pub end_time_unix_nano: String,
    pub attributes: Option<Vec<OtelKeyValue>>,
    /// Timestamped events recorded during the span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<OtelSpanEvent>>,
    /// Links to spans of the same or other traces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<OtelSpanLink>>,
    pub status: Option<OtelStatus>,
}

impl OtelSpan {
    /// A span with the required fields set and every optional field `None`;
    /// use with `..OtelSpan::new(..)` to set the rest.
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        name: impl Into<String>,
        start_time_unix_nano: impl Into<String>,
        end_time_unix_nano: impl Into<String>,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id: None,
            trace_state: None,
            name: name.into(),
            kind: None,
            start_time_unix_nano: start_time_unix_nano.into(),
            end_time_unix_nano: end_time_unix_nano.into(),
            attributes: None,
            events: None,
            links: None,
            status: None,
        }
    }
}

/// A timestamped event recorded during a span.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelSpanEvent {
    pub time_unix_nano: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelKeyValue>>,
}

/// A link from a span to another span.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelSpanLink {
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<OtelKeyValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelKeyValue {
    pub key: String,
    pub value: OtelAnyValue,
}

impl OtelKeyValue {
    /// An attribute with the given value.
    pub fn new(key: impl Into<String>, value: OtelAnyValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    /// A string attribute.
    pub fn string(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(key, OtelAnyValue::string(value))
    }

    /// An integer attribute.
    pub fn int(key: impl Into<String>, value: i64) -> Self {
        Self::new(key, OtelAnyValue::int(value))
    }

    /// A floating-point attribute.
    pub fn double(key: impl Into<String>, value: f64) -> Self {
        Self::new(key, OtelAnyValue::double(value))
    }

    /// A boolean attribute.
    pub fn bool(key: impl Into<String>, value: bool) -> Self {
        Self::new(key, OtelAnyValue::bool(value))
    }

    /// A string-list attribute (e.g. `langfuse.trace.tags`).
    pub fn strings<I, S>(key: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::new(key, OtelAnyValue::strings(values))
    }

    /// An attribute holding `value` serialized as a JSON string.
    pub fn json<T: Serialize + ?Sized>(
        key: impl Into<String>,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self::new(key, OtelAnyValue::json(value)?))
    }
}

/// An OTLP attribute value; set exactly one field (the constructors do).
#[derive(Debug, Clone, Default, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub array_value: Option<OtelArrayValue>,
}

impl OtelAnyValue {
    /// A string value.
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            string_value: Some(value.into()),
            ..Self::default()
        }
    }

    /// An integer value.
    pub fn int(value: i64) -> Self {
        Self {
            int_value: Some(value),
            ..Self::default()
        }
    }

    /// A floating-point value.
    pub fn double(value: f64) -> Self {
        Self {
            double_value: Some(value),
            ..Self::default()
        }
    }

    /// A boolean value.
    pub fn bool(value: bool) -> Self {
        Self {
            bool_value: Some(value),
            ..Self::default()
        }
    }

    /// An array of values.
    pub fn array(values: impl IntoIterator<Item = OtelAnyValue>) -> Self {
        Self {
            array_value: Some(OtelArrayValue {
                values: values.into_iter().collect(),
            }),
            ..Self::default()
        }
    }

    /// An array of string values.
    pub fn strings<I, S>(values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::array(values.into_iter().map(Self::string))
    }

    /// `value` serialized as a JSON string (how Langfuse expects input, output
    /// and nested metadata).
    pub fn json<T: Serialize + ?Sized>(value: &T) -> Result<Self, serde_json::Error> {
        serde_json::to_string(value).map(Self::string)
    }
}

/// The `arrayValue` of an [`OtelAnyValue`].
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelArrayValue {
    pub values: Vec<OtelAnyValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelStatus {
    pub code: Option<i32>,
    pub message: Option<String>,
}
