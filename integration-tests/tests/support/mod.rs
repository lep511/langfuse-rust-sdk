//! Helpers shared by the test files of this member: a tracer on a fake
//! Langfuse and the spans it exported, parsed from the OTLP/JSON requests.
//! They add the span helpers to the wiremock helpers of the main crate's
//! `tests/common`, which are re-exported.
#![allow(dead_code)]

#[cfg(feature = "bedrock")]
pub mod bedrock;
#[path = "../../../tests/common/mod.rs"]
mod common;

use std::time::Duration;

pub use common::*;
use langfuse_rust_sdk::otel::{LangfuseTracer, TracingOptions};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

/// Where the tracer posts its OTLP/JSON exports.
pub const OTEL_PATH: &str = "/api/public/otel/v1/traces";
/// Where the tracer asks for the upload URL of a media item.
pub const MEDIA_PATH: &str = "/api/public/media";

/// A tracer on a fake Langfuse that accepts every export. It only exports
/// when flushed (a one-hour flush interval), so each test reads one batch.
pub async fn tracer(options: TracingOptions) -> (LangfuseTracer, MockServer) {
    let (client, langfuse) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&langfuse)
        .await;
    let options = options.with_flush_interval(Duration::from_secs(3600));
    (client.tracer(options).unwrap(), langfuse)
}

/// Flush `tracer` and return every span the fake Langfuse received so far.
pub async fn exported(tracer: &LangfuseTracer, langfuse: &MockServer) -> Spans {
    tracer.flush().await;
    let mut spans = Vec::new();
    for request in langfuse.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body = body_json(&request);
        for resource_spans in body["resourceSpans"].as_array().unwrap() {
            for scope_spans in resource_spans["scopeSpans"].as_array().unwrap() {
                let scope = scope_spans["scope"]["name"].as_str().unwrap();
                for span in scope_spans["spans"].as_array().unwrap() {
                    spans.push(Span {
                        scope: scope.to_owned(),
                        json: span.clone(),
                    });
                }
            }
        }
    }
    Spans(spans)
}

/// The media upload requests (`POST /api/public/media`) the fake Langfuse
/// received. Uploads are queued by the export and sent by the flush.
pub async fn media_requests(langfuse: &MockServer) -> Vec<Value> {
    langfuse
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == MEDIA_PATH)
        .map(body_json)
        .collect()
}

/// The exported spans: grouped by instrumentation scope, in the order they
/// ended within a scope.
#[derive(Debug, Clone)]
pub struct Spans(Vec<Span>);

impl Spans {
    /// The only span named `name` (panics unless there is exactly one).
    pub fn named(&self, name: &str) -> &Span {
        let found = self.all_named(name);
        assert_eq!(
            found.len(),
            1,
            "expected one span named {name}, got {:#?}",
            self.0
        );
        found[0]
    }

    /// Every span named `name`, in the order they started.
    pub fn all_named(&self, name: &str) -> Vec<&Span> {
        let mut found: Vec<&Span> = self.0.iter().filter(|span| span.name() == name).collect();
        found.sort_by_key(|span| span.time_range().0);
        found
    }

    /// The span names, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.0.iter().map(Span::name).collect();
        names.sort_unstable();
        names
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Span> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One exported span: its OTLP/JSON and the name of its instrumentation scope.
#[derive(Debug, Clone)]
pub struct Span {
    pub scope: String,
    pub json: Value,
}

impl Span {
    pub fn name(&self) -> &str {
        self.json["name"].as_str().unwrap()
    }

    pub fn id(&self) -> &str {
        self.json["spanId"].as_str().unwrap()
    }

    pub fn trace_id(&self) -> &str {
        self.json["traceId"].as_str().unwrap()
    }

    /// The parent span ID, `None` for the root of a trace.
    pub fn parent_id(&self) -> Option<&str> {
        self.json["parentSpanId"]
            .as_str()
            .filter(|id| !id.is_empty())
    }

    /// Whether `parent` is the parent of this span, in the same trace.
    pub fn is_child_of(&self, parent: &Span) -> bool {
        self.trace_id() == parent.trace_id() && self.parent_id() == Some(parent.id())
    }

    /// An attribute as plain JSON: a string, number, bool or array of those.
    /// Panics when the key was exported more than once.
    pub fn attr(&self, key: &str) -> Option<Value> {
        let found: Vec<&Value> = self.json["attributes"]
            .as_array()?
            .iter()
            .filter(|kv| kv["key"] == key)
            .collect();
        assert!(
            found.len() <= 1,
            "attribute {key} exported {} times",
            found.len()
        );
        found.first().map(|kv| any_value(&kv["value"]))
    }

    /// A string attribute (panics when the attribute is not a string).
    pub fn text(&self, key: &str) -> Option<String> {
        self.attr(key).map(|value| match value {
            Value::String(text) => text,
            other => panic!("attribute {key} is not a string: {other}"),
        })
    }

    /// A string attribute parsed as JSON: the SDK serializes input, output,
    /// metadata, usage and model parameters. Plain text stays a string.
    pub fn json(&self, key: &str) -> Option<Value> {
        self.text(key)
            .map(|text| serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    /// `langfuse.observation.metadata.<key>`, parsed as JSON.
    pub fn metadata(&self, key: &str) -> Option<Value> {
        self.json(&format!("langfuse.observation.metadata.{key}"))
    }

    /// The names of the span's events, in order (for a `tracing` span: the
    /// messages of the events logged inside it).
    pub fn event_names(&self) -> Vec<&str> {
        let events = self.json["events"].as_array().map(Vec::as_slice);
        events
            .unwrap_or_default()
            .iter()
            .map(|event| event["name"].as_str().unwrap())
            .collect()
    }

    /// Start and end of the span, in nanoseconds since the Unix epoch.
    pub fn time_range(&self) -> (u128, u128) {
        let nanos = |field: &str| -> u128 {
            let value = &self.json[field];
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
                .parse()
                .unwrap_or_else(|_| panic!("{field} is not a timestamp: {value}"))
        };
        (nanos("startTimeUnixNano"), nanos("endTimeUnixNano"))
    }
}

/// An OTLP `AnyValue` as plain JSON.
fn any_value(value: &Value) -> Value {
    for scalar in ["stringValue", "intValue", "doubleValue", "boolValue"] {
        if let Some(inner) = value.get(scalar) {
            return inner.clone();
        }
    }
    if let Some(array) = value.get("arrayValue") {
        let values = array["values"].as_array().map(Vec::as_slice);
        return Value::Array(values.unwrap_or_default().iter().map(any_value).collect());
    }
    panic!("unexpected attribute value {value}");
}
