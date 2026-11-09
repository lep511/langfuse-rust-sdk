//! Tracing on OpenTelemetry (feature `otel`): observations, propagation and export.
#![cfg(feature = "otel")]

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use common::*;
use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
use langfuse_rust_sdk::models::prompts::Prompt;
use langfuse_rust_sdk::otel::opentelemetry::baggage::BaggageExt;
use langfuse_rust_sdk::otel::opentelemetry::trace::{
    Span as _, TraceContextExt, Tracer as _, TracerProvider as _,
};
use langfuse_rust_sdk::otel::opentelemetry::{Context, KeyValue};
use langfuse_rust_sdk::otel::{
    LangfuseTracer, Level, Observation, ObservationType, PromptLink, PropagatedAttributes,
    TraceContext, TracingOptions, current_observation_id, current_trace_id, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";

/// Options that only export on `flush`.
fn manual_flush() -> TracingOptions {
    TracingOptions::default().with_flush_interval(Duration::from_secs(3600))
}

async fn setup(options: TracingOptions) -> (LangfuseTracer, MockServer) {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let tracer = client.tracer(options).unwrap();
    (tracer, server)
}

/// One exported span with its scope and resource.
#[derive(Debug, Clone)]
struct Exported {
    span: Value,
    scope: Value,
    resource: Value,
}

impl Exported {
    fn name(&self) -> &str {
        self.span["name"].as_str().unwrap()
    }

    fn id(&self) -> &str {
        self.span["spanId"].as_str().unwrap()
    }

    fn trace_id(&self) -> &str {
        self.span["traceId"].as_str().unwrap()
    }

    fn parent_id(&self) -> Option<&str> {
        self.span["parentSpanId"].as_str()
    }

    /// Attribute value: string, number, bool, or array of those.
    fn attr(&self, key: &str) -> Option<Value> {
        attribute(&self.span["attributes"], key)
    }

    fn attr_str(&self, key: &str) -> Option<String> {
        self.attr(key).and_then(|v| v.as_str().map(str::to_owned))
    }

    /// Parse a JSON-encoded string attribute.
    fn attr_json(&self, key: &str) -> Option<Value> {
        self.attr_str(key)
            .map(|s| serde_json::from_str(&s).unwrap())
    }
}

fn any_value(value: &Value) -> Value {
    if let Some(s) = value.get("stringValue") {
        return s.clone();
    }
    if let Some(i) = value.get("intValue") {
        return i.clone();
    }
    if let Some(d) = value.get("doubleValue") {
        return d.clone();
    }
    if let Some(b) = value.get("boolValue") {
        return b.clone();
    }
    if let Some(array) = value.get("arrayValue") {
        return Value::Array(
            array["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(any_value)
                .collect(),
        );
    }
    panic!("unexpected attribute value {value}");
}

fn attribute(attributes: &Value, key: &str) -> Option<Value> {
    let matches: Vec<&Value> = attributes
        .as_array()?
        .iter()
        .filter(|kv| kv["key"] == key)
        .collect();
    assert!(
        matches.len() <= 1,
        "attribute {key} exported {} times",
        matches.len()
    );
    matches.first().map(|kv| any_value(&kv["value"]))
}

async fn export_requests(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == OTEL_PATH)
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

async fn exported(server: &MockServer) -> Vec<Exported> {
    let mut out = Vec::new();
    for body in export_requests(server).await {
        for resource_spans in body["resourceSpans"].as_array().unwrap() {
            for scope_spans in resource_spans["scopeSpans"].as_array().unwrap() {
                for span in scope_spans["spans"].as_array().unwrap() {
                    out.push(Exported {
                        span: span.clone(),
                        scope: scope_spans["scope"].clone(),
                        resource: resource_spans["resource"].clone(),
                    });
                }
            }
        }
    }
    out
}

fn named<'a>(spans: &'a [Exported], name: &str) -> &'a Exported {
    let found: Vec<&Exported> = spans.iter().filter(|s| s.name() == name).collect();
    assert_eq!(
        found.len(),
        1,
        "expected one span named {name}, got {spans:#?}"
    );
    found[0]
}

async fn wait_for_requests(server: &MockServer, count: usize) {
    for _ in 0..150 {
        if export_requests(server).await.len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("expected {count} export requests");
}

fn prompt(name: &str, version: i64) -> Prompt {
    serde_json::from_value(json!({
        "type": "text", "name": name, "version": version, "config": {},
        "labels": [], "tags": [], "prompt": "Hi"
    }))
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════
//  Observations
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn observation_attributes_are_exported() {
    let (tracer, server) = setup(manual_flush()).await;

    let span = tracer.start_observation("handle-request", ObservationType::Span);
    span.set_input(&json!({"question": "Where is my order?", "big": 9_007_199_254_740_993_u64}))
        .set_output("It ships tomorrow.")
        .set_metadata(&json!({"team": "search", "attempt": 2, "skip": null}))
        .set_version("v1.2")
        .set_level(Level::Warning)
        .set_status_message("served from cache");

    let generation = span.start_child("answer", ObservationType::Generation);
    generation
        .set_model("gpt-4o")
        .set_model_parameters(&json!({"temperature": 0.2}))
        .set_usage_details(&json!({"input": 120, "output": 45}))
        .set_cost_details(&json!({"total": 0.0012}))
        .set_completion_start_time(
            chrono::DateTime::parse_from_rfc3339("2024-06-15T10:30:00.5Z").unwrap(),
        )
        .set_prompt(&prompt("support", 3));
    generation.end();
    span.end();
    tracer.flush().await;

    let requests = server.received_requests().await.unwrap();
    let export = requests.iter().find(|r| r.url.path() == OTEL_PATH).unwrap();
    assert_eq!(export.headers["x-langfuse-ingestion-version"], "4");
    assert_eq!(export.headers["x-langfuse-public-key"], PUBLIC_KEY);

    let spans = exported(&server).await;
    assert_eq!(spans.len(), 2);
    let root = named(&spans, "handle-request");
    let child = named(&spans, "answer");

    assert_eq!(root.scope["name"], "langfuse-sdk");
    assert_eq!(root.scope["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        attribute(&root.scope["attributes"], "public_key"),
        Some(json!(PUBLIC_KEY))
    );

    assert_eq!(
        root.attr_str("langfuse.observation.type").as_deref(),
        Some("span")
    );
    assert_eq!(
        root.attr_json("langfuse.observation.input"),
        Some(json!({"question": "Where is my order?", "big": "9007199254740993"}))
    );
    assert_eq!(
        root.attr_str("langfuse.observation.output").as_deref(),
        Some("It ships tomorrow.")
    );
    assert_eq!(
        root.attr_str("langfuse.observation.metadata.team")
            .as_deref(),
        Some(r#""search""#)
    );
    assert_eq!(
        root.attr_str("langfuse.observation.metadata.attempt")
            .as_deref(),
        Some("2")
    );
    assert!(root.attr("langfuse.observation.metadata.skip").is_none());
    assert_eq!(root.attr_str("langfuse.version").as_deref(), Some("v1.2"));
    assert_eq!(
        root.attr_str("langfuse.observation.level").as_deref(),
        Some("WARNING")
    );
    assert_eq!(
        root.attr_str("langfuse.observation.status_message")
            .as_deref(),
        Some("served from cache")
    );
    assert!(root.parent_id().is_none());

    assert_eq!(child.trace_id(), root.trace_id());
    assert_eq!(child.parent_id(), Some(root.id()));
    assert_eq!(
        child.attr_str("langfuse.observation.type").as_deref(),
        Some("generation")
    );
    assert_eq!(
        child.attr_str("langfuse.observation.model.name").as_deref(),
        Some("gpt-4o")
    );
    assert_eq!(
        child.attr_json("langfuse.observation.model.parameters"),
        Some(json!({"temperature": 0.2}))
    );
    assert_eq!(
        child.attr_json("langfuse.observation.usage_details"),
        Some(json!({"input": 120, "output": 45}))
    );
    assert_eq!(
        child.attr_json("langfuse.observation.cost_details"),
        Some(json!({"total": 0.0012}))
    );
    assert_eq!(
        child.attr_json("langfuse.observation.completion_start_time"),
        Some(json!("2024-06-15T10:30:00.500000Z"))
    );
    assert_eq!(
        child
            .attr_str("langfuse.observation.prompt.name")
            .as_deref(),
        Some("support")
    );
    assert_eq!(
        child.attr("langfuse.observation.prompt.version"),
        Some(json!(3))
    );
}

#[tokio::test]
async fn generation_fields_are_ignored_on_spans_and_fallback_prompts_are_not_linked() {
    let (tracer, server) = setup(manual_flush()).await;
    let span = tracer.start_observation("tool-call", ObservationType::Tool);
    span.set_model("gpt-4o")
        .set_usage_details(&json!({"input": 1}));
    span.end();

    let mut fallback = prompt("support", 0);
    if let Prompt::Text { base, .. } = &mut fallback {
        base.is_fallback = true;
    }
    let generation = tracer.start_observation("gen", ObservationType::Embedding);
    generation
        .set_prompt(&fallback)
        .set_model("text-embedding-3-small");
    generation.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let tool = named(&spans, "tool-call");
    assert_eq!(
        tool.attr_str("langfuse.observation.type").as_deref(),
        Some("tool")
    );
    assert!(tool.attr("langfuse.observation.model.name").is_none());
    assert!(tool.attr("langfuse.observation.usage_details").is_none());
    let embedding = named(&spans, "gen");
    assert_eq!(
        embedding.attr_str("langfuse.observation.type").as_deref(),
        Some("embedding")
    );
    assert!(embedding.attr("langfuse.observation.prompt.name").is_none());
    assert_eq!(
        embedding
            .attr_str("langfuse.observation.model.name")
            .as_deref(),
        Some("text-embedding-3-small")
    );
}

#[tokio::test]
async fn updates_overwrite_and_merge_metadata() {
    let (tracer, server) = setup(manual_flush()).await;
    let span = tracer.start_observation("work", ObservationType::Chain);
    span.set_output("first")
        .set_output(&json!(["second"]))
        .set_metadata(&json!({"a": 1, "b": 1}))
        .set_metadata(&json!({"b": 2}))
        .set_name("renamed");
    span.end();
    span.set_output("after end is ignored");
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let span = named(&spans, "renamed");
    assert_eq!(
        span.attr_json("langfuse.observation.output"),
        Some(json!(["second"]))
    );
    assert_eq!(
        span.attr_str("langfuse.observation.metadata.a").as_deref(),
        Some("1")
    );
    assert_eq!(
        span.attr_str("langfuse.observation.metadata.b").as_deref(),
        Some("2")
    );
}

#[tokio::test]
async fn error_level_marks_the_span_as_failed() {
    let (tracer, server) = setup(manual_flush()).await;
    let span = tracer.start_observation("failing", ObservationType::Span);
    span.set_level(Level::Error)
        .set_status_message("timeout calling the model");
    span.end();
    let ok = tracer.start_observation("fine", ObservationType::Span);
    ok.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let failing = named(&spans, "failing");
    assert_eq!(failing.span["status"]["code"], 2);
    assert_eq!(
        failing.span["status"]["message"],
        "timeout calling the model"
    );
    assert_eq!(
        failing.attr_str("langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert_eq!(named(&spans, "fine").span["status"]["code"], 0);
}

#[tokio::test]
async fn events_end_where_they_start() {
    let (tracer, server) = setup(manual_flush()).await;
    let parent = tracer.start_observation("parent", ObservationType::Agent);
    let event = parent.start_child("cache-hit", ObservationType::Event);
    event.set_metadata(&json!({"key": "user:42"}));
    tokio::time::sleep(Duration::from_millis(5)).await;
    event.end();
    parent.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let event = named(&spans, "cache-hit");
    assert_eq!(
        event.attr_str("langfuse.observation.type").as_deref(),
        Some("event")
    );
    assert_eq!(
        event.span["startTimeUnixNano"],
        event.span["endTimeUnixNano"]
    );
    assert_eq!(event.parent_id(), Some(named(&spans, "parent").id()));
    assert_eq!(
        named(&spans, "parent")
            .attr_str("langfuse.observation.type")
            .as_deref(),
        Some("agent")
    );
}

#[tokio::test]
async fn dropping_the_last_handle_ends_the_observation() {
    let (tracer, server) = setup(manual_flush()).await;
    {
        let span = tracer.start_observation("dropped", ObservationType::Span);
        let clone = span.clone();
        clone.set_output("kept");
        assert!(!span.is_ended());
    }
    tracer.flush().await;
    let spans = exported(&server).await;
    assert_eq!(
        named(&spans, "dropped")
            .attr_str("langfuse.observation.output")
            .as_deref(),
        Some("kept")
    );
}

#[tokio::test]
async fn environment_and_release_are_recorded() {
    let (tracer, server) = setup(
        manual_flush()
            .with_environment("staging")
            .with_release("2024.06.1"),
    )
    .await;
    tracer
        .start_observation("work", ObservationType::Span)
        .end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let span = named(&spans, "work");
    assert_eq!(
        span.attr_str("langfuse.environment").as_deref(),
        Some("staging")
    );
    assert_eq!(
        span.attr_str("langfuse.release").as_deref(),
        Some("2024.06.1")
    );
    assert_eq!(
        attribute(&span.resource["attributes"], "langfuse.environment"),
        Some(json!("staging"))
    );
    assert_eq!(
        attribute(&span.resource["attributes"], "langfuse.release"),
        Some(json!("2024.06.1"))
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Context
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn in_scope_makes_the_observation_current() {
    let (tracer, server) = setup(manual_flush()).await;
    let root = tracer.start_observation("root", ObservationType::Span);
    assert!(Observation::current().is_none());
    assert!(current_trace_id().is_none());

    let (trace_id, observation_id) = root
        .in_scope(async {
            let current = Observation::current().expect("current observation");
            assert_eq!(current.id(), root.id());
            tokio::task::yield_now().await;
            let child = tracer.start_observation("child", ObservationType::Tool);
            child
                .in_scope(async {
                    tracer
                        .start_observation("grandchild", ObservationType::Span)
                        .end()
                })
                .await;
            child.end();
            (current_trace_id(), current_observation_id())
        })
        .await;
    root.end();
    assert_eq!(trace_id.as_deref(), Some(root.trace_id()));
    assert_eq!(observation_id.as_deref(), Some(root.id()));
    assert!(Observation::current().is_none());
    tracer.flush().await;

    let spans = exported(&server).await;
    let child = named(&spans, "child");
    assert_eq!(child.parent_id(), Some(root.id()));
    assert_eq!(named(&spans, "grandchild").parent_id(), Some(child.id()));
}

#[tokio::test]
async fn propagated_attributes_reach_the_current_and_new_spans() {
    let (tracer, server) = setup(manual_flush()).await;
    let root = tracer.start_observation("root", ObservationType::Span);
    let outer = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_session_id("session-7")
        .with_tag("beta")
        .with_trace_name("checkout")
        .with_version("v3")
        .with_metadata("tenant", "acme");
    let inner = PropagatedAttributes::default()
        .with_tags(["beta", "eu"])
        .with_prompt_link(PromptLink {
            name: "support".into(),
            version: 4,
        });
    root.in_scope(propagate_attributes(outer, async {
        tracer
            .start_observation("child", ObservationType::Span)
            .end();
        propagate_attributes(inner, async {
            tracer
                .start_observation("grandchild", ObservationType::Generation)
                .end();
        })
        .await;
    }))
    .await;
    root.end();
    // Outside the scope nothing is propagated.
    tracer
        .start_observation("unrelated", ObservationType::Span)
        .end();
    tracer.flush().await;

    let spans = exported(&server).await;
    for name in ["root", "child", "grandchild"] {
        let span = named(&spans, name);
        assert_eq!(
            span.attr_str("user.id").as_deref(),
            Some("user-42"),
            "{name}"
        );
        assert_eq!(
            span.attr_str("session.id").as_deref(),
            Some("session-7"),
            "{name}"
        );
        assert_eq!(
            span.attr_str("langfuse.trace.name").as_deref(),
            Some("checkout"),
            "{name}"
        );
        assert_eq!(
            span.attr_str("langfuse.version").as_deref(),
            Some("v3"),
            "{name}"
        );
        assert_eq!(
            span.attr_str("langfuse.trace.metadata.tenant").as_deref(),
            Some(r#""acme""#),
            "{name}"
        );
    }
    assert_eq!(
        named(&spans, "child").attr("langfuse.trace.tags"),
        Some(json!(["beta"]))
    );
    let grandchild = named(&spans, "grandchild");
    assert_eq!(
        grandchild.attr("langfuse.trace.tags"),
        Some(json!(["beta", "eu"]))
    );
    assert_eq!(
        grandchild
            .attr_str("langfuse.observation.prompt.name")
            .as_deref(),
        Some("support")
    );
    assert_eq!(
        grandchild.attr("langfuse.observation.prompt.version"),
        Some(json!(4))
    );
    assert!(named(&spans, "unrelated").attr("user.id").is_none());
}

#[tokio::test]
async fn scopes_nest_in_either_order() {
    let (tracer, server) = setup(manual_flush()).await;
    let root = tracer.start_observation("root", ObservationType::Span);
    let attributes = PropagatedAttributes::default().with_session_id("s-1");
    // `in_scope` inside `propagate_attributes`: the scope keeps the attributes.
    propagate_attributes(
        attributes,
        root.in_scope(async {
            tracer
                .start_observation("child", ObservationType::Span)
                .end();
        }),
    )
    .await;
    root.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let child = named(&spans, "child");
    assert_eq!(child.parent_id(), Some(named(&spans, "root").id()));
    assert_eq!(child.attr_str("session.id").as_deref(), Some("s-1"));
}

#[tokio::test]
async fn start_child_inherits_the_attributes_of_the_current_scope() {
    let (tracer, server) = setup(manual_flush()).await;
    let root = tracer.start_observation("root", ObservationType::Span);
    let attributes = PropagatedAttributes::default().with_user_id("user-1");
    propagate_attributes(attributes, async {
        root.start_child("child", ObservationType::Span).end();
    })
    .await;
    root.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let child = named(&spans, "child");
    assert_eq!(child.parent_id(), Some(named(&spans, "root").id()));
    assert_eq!(child.attr_str("user.id").as_deref(), Some("user-1"));
}

#[tokio::test]
async fn propagation_as_baggage_and_environment_override() {
    let (tracer, server) = setup(manual_flush().with_environment("production")).await;
    let attributes = PropagatedAttributes::default()
        .with_user_id("user-9")
        .with_environment("canary")
        .with_baggage(true);
    let baggage_user = propagate_attributes(attributes, async {
        tracer
            .start_observation("in-scope", ObservationType::Span)
            .end();
        Context::current()
            .baggage()
            .get("langfuse_user_id")
            .map(|v| v.as_str().to_owned())
    })
    .await;
    assert_eq!(baggage_user.as_deref(), Some("user-9"));

    // Baggage received from another service (extracted into the context) applies too.
    let incoming = Context::current().with_baggage([
        KeyValue::new("langfuse_session_id", "remote-session"),
        KeyValue::new("langfuse_tags", "a,b"),
    ]);
    {
        let _guard = incoming.attach();
        tracer
            .start_observation("from-baggage", ObservationType::Span)
            .end();
    }
    tracer.flush().await;

    let spans = exported(&server).await;
    let in_scope = named(&spans, "in-scope");
    assert_eq!(
        in_scope.attr_str("langfuse.environment").as_deref(),
        Some("canary")
    );
    let remote = named(&spans, "from-baggage");
    assert_eq!(
        remote.attr_str("session.id").as_deref(),
        Some("remote-session")
    );
    assert_eq!(remote.attr("langfuse.trace.tags"), Some(json!(["a", "b"])));
    assert_eq!(
        remote.attr_str("langfuse.environment").as_deref(),
        Some("production")
    );
}

#[tokio::test]
async fn trace_context_continues_an_existing_trace() {
    let (tracer, server) = setup(manual_flush()).await;
    let trace_id = create_trace_id(Some("external-request-1"));
    let parent_id = create_observation_id(None);

    tracer
        .start_observation_in_trace(
            "continued",
            ObservationType::Span,
            &TraceContext {
                trace_id: trace_id.clone(),
                parent_observation_id: Some(parent_id.clone()),
            },
        )
        .end();
    tracer
        .start_observation_in_trace(
            "new-root",
            ObservationType::Span,
            &TraceContext::new(trace_id.clone()),
        )
        .end();
    tracer
        .start_observation_in_trace(
            "invalid",
            ObservationType::Span,
            &TraceContext::new("not-a-trace-id"),
        )
        .end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let continued = named(&spans, "continued");
    assert_eq!(continued.trace_id(), trace_id);
    assert_eq!(continued.parent_id(), Some(parent_id.as_str()));
    assert_eq!(
        continued.attr("langfuse.internal.as_root"),
        Some(json!(true))
    );
    let new_root = named(&spans, "new-root");
    assert_eq!(new_root.trace_id(), trace_id);
    assert_eq!(
        new_root.attr("langfuse.internal.as_root"),
        Some(json!(true))
    );
    let invalid = named(&spans, "invalid");
    assert_ne!(invalid.trace_id(), trace_id);
    assert!(invalid.attr("langfuse.internal.as_root").is_none());
}

// ═══════════════════════════════════════════════════════════════════════
//  Export control
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn zero_sample_rate_records_nothing() {
    let (tracer, server) = setup(manual_flush().with_sample_rate(0.0)).await;
    let span = tracer.start_observation("sampled-out", ObservationType::Span);
    assert!(!span.is_recording());
    span.set_input("ignored");
    span.end();
    tracer.flush().await;
    assert!(export_requests(&server).await.is_empty());
}

#[tokio::test]
async fn disabled_tracing_is_a_no_op() {
    let (tracer, server) = setup(manual_flush().with_enabled(false)).await;
    let span = tracer.start_observation("disabled", ObservationType::Span);
    assert!(!span.is_recording());
    span.end();
    tracer.flush().await;
    tracer.shutdown().await;
    assert!(export_requests(&server).await.is_empty());
}

#[test]
fn an_enabled_tracer_needs_a_tokio_runtime() {
    let client = LangfuseClient::new(ClientConfig::builder("pk", "sk").build().unwrap()).unwrap();
    match client.tracer(TracingOptions::default()) {
        Err(LangfuseError::Config(msg)) => assert!(msg.contains("Tokio runtime"), "{msg}"),
        other => panic!("expected Config error, got {other:?}"),
    }
    let disabled = TracingOptions::default().with_enabled(false);
    assert!(client.tracer(disabled).is_ok());
    for invalid in [
        TracingOptions::default()
            .with_sample_rate(1.5)
            .with_enabled(false),
        TracingOptions::default()
            .with_flush_at(0)
            .with_enabled(false),
    ] {
        assert!(matches!(
            client.tracer(invalid),
            Err(LangfuseError::Config(_))
        ));
    }
}

#[tokio::test]
async fn mask_redacts_input_output_and_metadata() {
    let mask = |value: Value| match value {
        Value::String(s) if s.contains("secret") => Value::String("[REDACTED]".into()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    if k == "password" {
                        (k, json!("***"))
                    } else {
                        (k, v)
                    }
                })
                .collect(),
        ),
        other => other,
    };
    let (tracer, server) = setup(manual_flush().with_mask(mask)).await;
    let span = tracer.start_observation("login", ObservationType::Span);
    span.set_input(&json!({"user": "ada", "password": "hunter2"}))
        .set_output("the secret is 42")
        .set_metadata(&json!({"password": "x", "ip": "10.0.0.1"}));
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let span = named(&spans, "login");
    assert_eq!(
        span.attr_json("langfuse.observation.input"),
        Some(json!({"user": "ada", "password": "***"}))
    );
    assert_eq!(
        span.attr_str("langfuse.observation.output").as_deref(),
        Some("[REDACTED]")
    );
    assert_eq!(
        span.attr_str("langfuse.observation.metadata.password")
            .as_deref(),
        Some(r#""***""#)
    );
}

#[tokio::test]
async fn a_panicking_mask_masks_everything() {
    let (tracer, server) = setup(manual_flush().with_mask(|_| panic!("bad mask"))).await;
    let span = tracer.start_observation("masked", ObservationType::Span);
    span.set_input("sensitive");
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    assert_eq!(
        named(&spans, "masked")
            .attr_str("langfuse.observation.input")
            .as_deref(),
        Some("<fully masked due to failed mask function>")
    );
}

#[tokio::test]
async fn foreign_spans_are_filtered_by_default() {
    let (tracer, server) = setup(manual_flush()).await;
    let http = tracer.provider().tracer("my-http-lib");
    http.start("GET /health").end();
    http.span_builder("llm-call")
        .with_attributes([KeyValue::new("gen_ai.system", "openai")])
        .start(&http)
        .end();
    tracer
        .provider()
        .tracer("opentelemetry.instrumentation.openai.v2")
        .start("chat")
        .end();
    tracer
        .provider()
        .tracer("opentelemetry.instrumentation.openaiX")
        .start("lookalike")
        .end();
    tracer.flush().await;

    let names: Vec<String> = exported(&server)
        .await
        .iter()
        .map(|s| s.name().to_owned())
        .collect();
    assert!(names.contains(&"llm-call".to_string()), "{names:?}");
    assert!(names.contains(&"chat".to_string()), "{names:?}");
    assert!(!names.contains(&"GET /health".to_string()), "{names:?}");
    assert!(!names.contains(&"lookalike".to_string()), "{names:?}");
}

#[tokio::test]
async fn should_export_span_overrides_the_default_filter() {
    let (tracer, server) =
        setup(manual_flush().with_should_export_span(|span| span.name != "noisy")).await;
    tracer
        .provider()
        .tracer("my-http-lib")
        .start("GET /orders")
        .end();
    tracer
        .start_observation("noisy", ObservationType::Span)
        .end();
    tracer.flush().await;

    let names: Vec<String> = exported(&server)
        .await
        .iter()
        .map(|s| s.name().to_owned())
        .collect();
    assert_eq!(names, ["GET /orders"]);
}

#[tokio::test]
async fn the_first_exported_span_is_marked_as_application_root() {
    let (tracer, server) = setup(manual_flush()).await;
    // A framework span that is not exported, with a Langfuse observation inside.
    let framework = tracer.provider().tracer("my-web-framework");
    let request = framework.start("POST /chat");
    let cx = Context::current_with_span(request);
    {
        let _guard = cx.clone().attach();
        let handler = tracer.start_observation("handler", ObservationType::Span);
        handler
            .start_child("llm", ObservationType::Generation)
            .end();
        handler.end();
    }
    cx.span().end();
    tracer.flush().await;

    let spans = exported(&server).await;
    assert_eq!(spans.len(), 2, "the framework span is filtered out");
    let handler = named(&spans, "handler");
    assert!(handler.parent_id().is_some());
    assert_eq!(
        handler.attr("langfuse.internal.is_app_root"),
        Some(json!(true))
    );
    assert!(
        named(&spans, "llm")
            .attr("langfuse.internal.is_app_root")
            .is_none()
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Batching
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn flush_at_exports_without_an_explicit_flush() {
    let (tracer, server) = setup(manual_flush().with_flush_at(2)).await;
    tracer.start_observation("one", ObservationType::Span).end();
    tracer.start_observation("two", ObservationType::Span).end();
    wait_for_requests(&server, 1).await;
    assert_eq!(exported(&server).await.len(), 2);
}

#[tokio::test]
async fn flush_interval_exports_periodically() {
    let (tracer, server) =
        setup(TracingOptions::default().with_flush_interval(Duration::from_millis(50))).await;
    tracer
        .start_observation("periodic", ObservationType::Span)
        .end();
    wait_for_requests(&server, 1).await;
    named(&exported(&server).await, "periodic");
}

#[tokio::test]
async fn large_batches_are_split_by_size() {
    let (tracer, server) = setup(manual_flush().with_max_batch_bytes(1500)).await;
    for i in 0..3 {
        let span = tracer.start_observation(format!("span-{i}"), ObservationType::Span);
        span.set_input(&"x".repeat(1000));
        span.end();
    }
    tracer.flush().await;
    assert_eq!(export_requests(&server).await.len(), 3);
    assert_eq!(exported(&server).await.len(), 3);
}

#[tokio::test]
async fn shutdown_exports_queued_spans_and_stops() {
    let (tracer, server) = setup(manual_flush()).await;
    tracer
        .start_observation("before", ObservationType::Span)
        .end();
    tracer.shutdown().await;
    tracer
        .start_observation("after", ObservationType::Span)
        .end();
    tracer.flush().await;
    tracer.shutdown().await;

    let names: Vec<String> = exported(&server)
        .await
        .iter()
        .map(|s| s.name().to_owned())
        .collect();
    assert_eq!(names, ["before"]);
}

#[tokio::test]
async fn export_uses_basic_auth_and_retries_server_errors() {
    let (client, server) = test_client_with_retries(1).await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .and(header("authorization", "Basic cGstdGVzdDpzay10ZXN0"))
        .respond_with(json(502, json!({"message": "bad gateway"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let tracer = client.tracer(manual_flush()).unwrap();
    tracer
        .start_observation("retried", ObservationType::Span)
        .end();
    tracer.flush().await;
    assert_eq!(export_requests(&server).await.len(), 2);
}

#[tokio::test]
async fn spans_with_many_updates_keep_one_value_per_key() {
    let (tracer, server) = setup(manual_flush()).await;
    let span = tracer.start_observation("streaming", ObservationType::Generation);
    for i in 0..500 {
        span.set_output(&format!("token {i}"));
    }
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let span = named(&spans, "streaming");
    assert_eq!(
        span.attr_str("langfuse.observation.output").as_deref(),
        Some("token 499")
    );
    let keys: BTreeMap<String, usize> =
        span.span["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .fold(BTreeMap::new(), |mut acc, kv| {
                *acc.entry(kv["key"].as_str().unwrap().to_owned())
                    .or_default() += 1;
                acc
            });
    assert!(keys.values().all(|n| *n == 1), "{keys:?}");
}

// ═══════════════════════════════════════════════════════════════════════
//  Scores and media
// ═══════════════════════════════════════════════════════════════════════

use langfuse_rust_sdk::media::LangfuseMedia;
use langfuse_rust_sdk::models::media::MediaContentType;
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};

const PNG_B64: &str = "iVBORw0KGgo=";

async fn accept_scores_and_media(server: &MockServer, media_id: &str) {
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(202, json!({"message": "Accepted"})))
        .mount(server)
        .await;
    // Already stored: the upload stops after requesting the URL.
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(200, json!({"uploadUrl": null, "mediaId": media_id})))
        .mount(server)
        .await;
}

async fn sent_scores(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/api/public/scores")
        .flat_map(|r| serde_json::from_slice::<Vec<Value>>(&r.body).unwrap())
        .collect()
}

async fn media_requests(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/api/public/media")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn score(name: &str) -> CreateScoreRequest {
    CreateScoreRequest::new(name, CreateScoreValue::Numeric(1.0))
}

#[tokio::test]
async fn observations_score_themselves_and_their_trace() {
    let (tracer, server) = setup(manual_flush().with_environment("staging")).await;
    accept_scores_and_media(&server, "unused").await;

    let span = tracer.start_observation("answer", ObservationType::Generation);
    span.score(score("correctness"));
    span.score_trace(CreateScoreRequest {
        observation_id: Some("ignored".into()),
        environment: Some("explicit".into()),
        ..score("user-feedback")
    });
    span.end();
    tracer.flush().await;

    let scores = sent_scores(&server).await;
    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0]["name"], "correctness");
    assert_eq!(scores[0]["traceId"], span.trace_id());
    assert_eq!(scores[0]["observationId"], span.id());
    assert_eq!(scores[0]["environment"], "staging");
    assert_eq!(scores[1]["traceId"], span.trace_id());
    assert!(scores[1].get("observationId").is_none());
    assert_eq!(scores[1]["environment"], "explicit");
}

#[tokio::test]
async fn score_current_span_and_trace() {
    let (tracer, server) = setup(manual_flush()).await;
    accept_scores_and_media(&server, "unused").await;

    let span = tracer.start_observation("work", ObservationType::Span);
    span.in_scope(async {
        tracer.score_current_span(score("span-level"));
        tracer.score_current_trace(score("trace-level"));
    })
    .await;
    span.end();
    tracer.score_current_span(score("no-current-span"));
    tracer.flush().await;

    let scores = sent_scores(&server).await;
    let names: Vec<&str> = scores.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["span-level", "trace-level"]);
    assert_eq!(scores[0]["observationId"], span.id());
    assert!(scores[1].get("observationId").is_none());
}

#[tokio::test]
async fn scores_of_unsampled_observations_are_dropped() {
    let (tracer, server) = setup(manual_flush().with_sample_rate(0.0)).await;
    accept_scores_and_media(&server, "unused").await;
    let span = tracer.start_observation("sampled-out", ObservationType::Span);
    span.score(score("dropped"));
    span.end();
    tracer.flush().await;
    assert!(sent_scores(&server).await.is_empty());
}

#[tokio::test]
async fn tracer_score_follows_the_sampling_of_its_trace() {
    let (tracer, server) = setup(
        manual_flush()
            .with_sample_rate(0.5)
            .with_environment("staging"),
    )
    .await;
    accept_scores_and_media(&server, "unused").await;

    // A span and a score of the same trace get the same decision.
    let mut recorded = Vec::new();
    for i in 1..=40u128 {
        let trace_id = format!(
            "{:032x}",
            i.wrapping_mul(0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835)
        );
        let span = tracer.start_observation_in_trace(
            "probe",
            ObservationType::Span,
            &TraceContext::new(trace_id.clone()),
        );
        if span.is_recording() {
            recorded.push(trace_id.clone());
        }
        span.end();
        tracer.score(CreateScoreRequest {
            trace_id: Some(trace_id),
            ..score("probe")
        });
    }
    assert!(
        !recorded.is_empty() && recorded.len() < 40,
        "both decisions occur ({} of 40 recorded)",
        recorded.len()
    );
    tracer.flush().await;

    let scores = sent_scores(&server).await;
    let sent: Vec<&str> = scores
        .iter()
        .map(|s| s["traceId"].as_str().unwrap())
        .collect();
    assert_eq!(sent, recorded);
    assert!(scores.iter().all(|s| s["environment"] == "staging"));
}

#[tokio::test]
async fn tracer_score_keeps_scores_without_a_w3c_trace_id() {
    let (tracer, server) = setup(manual_flush().with_sample_rate(0.0)).await;
    accept_scores_and_media(&server, "unused").await;
    tracer.score(CreateScoreRequest {
        trace_id: Some("0af7651916cd43dd8448eb211c80319c".into()),
        ..score("w3c")
    });
    tracer.score(CreateScoreRequest {
        trace_id: Some("my-trace-42".into()),
        ..score("custom-id")
    });
    tracer.score(CreateScoreRequest {
        session_id: Some("session-7".into()),
        environment: Some("eval".into()),
        ..score("session")
    });
    tracer.flush().await;

    let scores = sent_scores(&server).await;
    let names: Vec<&str> = scores.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["custom-id", "session"]);
    assert_eq!(
        scores[1]["environment"], "eval",
        "an explicit environment is kept"
    );
}

#[tokio::test]
async fn tracer_score_is_dropped_when_tracing_is_disabled() {
    let (tracer, server) = setup(manual_flush().with_enabled(false)).await;
    accept_scores_and_media(&server, "unused").await;
    tracer.score(CreateScoreRequest {
        session_id: Some("session-7".into()),
        ..score("ignored")
    });
    tracer.flush().await;
    assert!(sent_scores(&server).await.is_empty());
}

#[tokio::test]
async fn base64_media_in_observation_data_is_uploaded_and_referenced() {
    let data_uri = format!("data:image/png;base64,{PNG_B64}");
    let media = LangfuseMedia::from_data_uri(&data_uri).unwrap();
    let reference = media.reference_string();
    let (tracer, server) = setup(manual_flush()).await;
    accept_scores_and_media(&server, media.media_id()).await;

    let span = tracer.start_observation("vision", ObservationType::Generation);
    span.set_input(&json!({"image": data_uri, "prompt": "describe"}))
        .set_output(&json!([{"type": "base64", "media_type": "image/png", "data": PNG_B64}]))
        .set_metadata(&json!({"file": data_uri}));
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let exported_span = named(&spans, "vision");
    assert_eq!(
        exported_span.attr_json("langfuse.observation.input"),
        Some(json!({"image": reference, "prompt": "describe"}))
    );
    assert_eq!(
        exported_span
            .attr_json("langfuse.observation.output")
            .unwrap()[0]["data"],
        json!(reference)
    );
    assert_eq!(
        exported_span.attr_json("langfuse.observation.metadata.file"),
        Some(json!(reference))
    );

    let uploads = media_requests(&server).await;
    let mut fields: Vec<&str> = uploads
        .iter()
        .map(|u| u["field"].as_str().unwrap())
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["input", "metadata", "output"]);
    for upload in &uploads {
        assert_eq!(upload["traceId"], span.trace_id());
        assert_eq!(upload["observationId"], span.id());
        assert_eq!(upload["contentType"], "image/png");
        assert_eq!(upload["sha256Hash"], media.sha256_hash());
    }
}

#[tokio::test]
async fn langfuse_media_values_are_uploaded_from_observation_data() {
    let media =
        LangfuseMedia::from_bytes(b"%PDF-1.4 tiny".to_vec(), MediaContentType::ApplicationPdf);
    let (tracer, server) = setup(manual_flush()).await;
    accept_scores_and_media(&server, media.media_id()).await;

    let span = tracer.start_observation("summarize", ObservationType::Span);
    span.set_input(&json!({"document": media}));
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let input = named(&spans, "summarize")
        .attr_json("langfuse.observation.input")
        .unwrap();
    let reference = input["document"].as_str().unwrap();
    assert!(
        reference.starts_with("@@@langfuseMedia:type=application/pdf|id="),
        "{reference}"
    );
    assert!(reference.contains(media.media_id()));
    assert_eq!(media_requests(&server).await.len(), 1);
}

#[tokio::test]
async fn media_in_other_instrumentation_spans_is_uploaded() {
    let media = LangfuseMedia::from_data_uri(&format!("data:image/png;base64,{PNG_B64}")).unwrap();
    let (tracer, server) = setup(manual_flush()).await;
    accept_scores_and_media(&server, media.media_id()).await;

    let prompt = json!([{"role": "user", "parts": [{"inline_data": {"mime_type": "image/png", "data": PNG_B64}}]}]);
    let llm = tracer
        .provider()
        .tracer("opentelemetry.instrumentation.google_generativeai");
    llm.span_builder("generate")
        .with_attributes([KeyValue::new("gen_ai.prompt", prompt.to_string())])
        .start(&llm)
        .end();
    tracer.flush().await;

    let spans = exported(&server).await;
    let rewritten = named(&spans, "generate")
        .attr_json("gen_ai.prompt")
        .unwrap();
    assert_eq!(
        rewritten[0]["parts"][0]["inline_data"]["data"],
        json!(media.reference_string())
    );
    let uploads = media_requests(&server).await;
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0]["field"], "input");
}

#[tokio::test]
async fn media_upload_can_be_disabled() {
    let data_uri = format!("data:image/png;base64,{PNG_B64}");
    let (tracer, server) = setup(manual_flush().with_media_upload(false)).await;
    accept_scores_and_media(&server, "unused").await;

    let span = tracer.start_observation("raw", ObservationType::Span);
    span.set_input(&data_uri);
    span.end();
    tracer.flush().await;

    let spans = exported(&server).await;
    assert_eq!(
        named(&spans, "raw").attr_str("langfuse.observation.input"),
        Some(data_uri)
    );
    assert!(media_requests(&server).await.is_empty());
}
