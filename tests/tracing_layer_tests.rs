//! `otel::tracing_layer` (feature `tracing-layer`): `tracing` spans, and rig's
//! `gen_ai.*` spans, exported to Langfuse.
#![cfg(feature = "tracing-layer")]

mod common;

use std::time::Duration;

use common::*;
use langfuse_rust_sdk::otel::{
    self, LangfuseTracer, ObservationType, TRACING_LAYER_SCOPE, TracingOptions,
};
use rig_agent::prelude::*;
use rig_core::completion::CompletionRequest;
use rig_core::providers::openai::OpenAIConfig;
use serde_json::{Value, json};
use tracing::Instrument as _;
use tracing_subscriber::prelude::*;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";

async fn setup(options: TracingOptions) -> (LangfuseTracer, MockServer) {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let options = options.with_flush_interval(Duration::from_secs(3600));
    (client.tracer(options).unwrap(), server)
}

/// Exported spans as `(scope, span)` pairs.
async fn exported(server: &MockServer) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body: Value = body_json(&request);
        for resource_spans in body["resourceSpans"].as_array().unwrap() {
            for scope_spans in resource_spans["scopeSpans"].as_array().unwrap() {
                let scope = scope_spans["scope"]["name"].as_str().unwrap().to_owned();
                for span in scope_spans["spans"].as_array().unwrap() {
                    out.push((scope.clone(), span.clone()));
                }
            }
        }
    }
    out
}

fn named<'a>(spans: &'a [(String, Value)], name: &str) -> &'a (String, Value) {
    let found: Vec<_> = spans.iter().filter(|(_, s)| s["name"] == name).collect();
    assert_eq!(found.len(), 1, "expected one span named {name}: {spans:#?}");
    found[0]
}

fn attr(span: &Value, key: &str) -> Option<Value> {
    let kv = span["attributes"]
        .as_array()?
        .iter()
        .find(|kv| kv["key"] == key)?;
    let value = &kv["value"];
    ["stringValue", "intValue", "doubleValue", "boolValue"]
        .iter()
        .find_map(|k| value.get(*k).cloned())
}

/// A fake OpenAI answering every chat completion with "Paris".
async fn fake_openai() -> MockServer {
    let openai = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex("chat/completions$"))
        .respond_with(json(
            200,
            json!({
                "id": "chatcmpl-1", "object": "chat.completion", "created": 1,
                "model": "gpt-4o-mini-2024-07-18",
                "choices": [{"index": 0, "finish_reason": "stop",
                             "message": {"role": "assistant", "content": "Paris"}}],
                "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
            }),
        ))
        .mount(&openai)
        .await;
    openai
}

#[tokio::test]
async fn rig_agents_become_children_of_the_current_observation() {
    let (tracer, server) = setup(TracingOptions::default()).await;
    let openai = fake_openai().await;
    let _subscriber = tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer))
        .set_default();

    let model = OpenAIConfig::new("sk-test")
        .with_base_url(openai.uri())
        .client()
        .chat("gpt-4o-mini");
    let agent = AgentBuilder::new(model)
        .preamble("Answer with one word.")
        .record_content_telemetry(true)
        .build();
    let root = tracer.start_observation("answer-question", ObservationType::Span);
    // rig-agent starts a new trace unless a `tracing` span is current: it
    // adopts that span as the agent span. Created inside the observation's
    // scope, the span is the observation's child.
    let answer = root
        .in_scope(async {
            let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt("Capital of France?").await }
                .instrument(span)
                .await
        })
        .await
        .unwrap();
    root.end();
    assert_eq!(answer.output(), "Paris");
    tracer.flush().await;

    let spans = exported(&server).await;
    let (_, root) = named(&spans, "answer-question");
    let (scope, agent) = named(&spans, "geo-agent");
    assert_eq!(scope, TRACING_LAYER_SCOPE);
    assert_eq!(agent["traceId"], root["traceId"]);
    assert_eq!(agent["parentSpanId"], root["spanId"]);
    let (_, chat) = named(&spans, "chat");
    assert_eq!(chat["traceId"], root["traceId"]);
    assert_eq!(chat["parentSpanId"], agent["spanId"]);
    assert_eq!(
        attr(chat, "gen_ai.request.model"),
        Some(json!("gpt-4o-mini"))
    );
    assert_eq!(
        attr(chat, "gen_ai.response.model"),
        Some(json!("gpt-4o-mini-2024-07-18"))
    );
    // `tracing` records unsigned integers through `Debug`: as strings.
    assert_eq!(attr(chat, "gen_ai.usage.input_tokens"), Some(json!("12")));
    assert_eq!(attr(chat, "gen_ai.usage.output_tokens"), Some(json!("3")));
    let text = |key: &str| attr(chat, key).unwrap().as_str().unwrap().to_owned();
    assert!(text("gen_ai.input.messages").contains("Capital of France?"));
    assert!(text("gen_ai.output.messages").contains("Paris"));
    assert!(text("gen_ai.system_instructions").contains("Answer with one word."));
    // No busy/idle timings.
    assert_eq!(attr(chat, "busy_ns"), None);
}

#[tokio::test]
async fn direct_model_calls_record_model_and_usage() {
    let (tracer, server) = setup(TracingOptions::default()).await;
    let openai = fake_openai().await;
    let _subscriber = tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer))
        .set_default();
    let model = OpenAIConfig::new("sk-test")
        .with_base_url(openai.uri())
        .client()
        .chat("gpt-4o-mini");
    model
        .call(CompletionRequest::new("Capital of France?"))
        .await
        .unwrap();
    tracer.flush().await;

    let spans = exported(&server).await;
    let (_, chat) = named(&spans, "chat");
    assert_eq!(
        attr(chat, "gen_ai.request.model"),
        Some(json!("gpt-4o-mini"))
    );
    assert_eq!(attr(chat, "gen_ai.usage.output_tokens"), Some(json!("3")));
}

#[tokio::test]
async fn observations_nest_under_entered_tracing_spans() {
    let options = TracingOptions::default().with_should_export_span(|span| {
        otel::is_default_export_span(span) || otel::is_tracing_layer_span(span)
    });
    let (tracer, server) = setup(options).await;
    let _subscriber = tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer))
        .set_default();

    {
        let span = tracing::info_span!("handle_request", user = "u-1");
        let _entered = span.enter();
        tracer
            .start_observation("retrieve", ObservationType::Retriever)
            .end();
    }
    // Without the filter's help, plain tracing spans are not exported.
    tracer.flush().await;

    let spans = exported(&server).await;
    let (scope, request) = named(&spans, "handle_request");
    assert_eq!(scope, TRACING_LAYER_SCOPE);
    assert_eq!(attr(request, "user"), Some(json!("u-1")));
    let (_, retrieve) = named(&spans, "retrieve");
    assert_eq!(retrieve["parentSpanId"], request["spanId"]);
    assert_eq!(retrieve["traceId"], request["traceId"]);
}

#[tokio::test]
async fn plain_tracing_spans_are_not_exported_by_default() {
    let (tracer, server) = setup(TracingOptions::default()).await;
    let _subscriber = tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer))
        .set_default();
    tracing::info_span!("db_query").in_scope(|| {});
    tracing::info_span!("llm", gen_ai.operation.name = "chat").in_scope(|| {});
    tracer.flush().await;

    let names: Vec<Value> = exported(&server)
        .await
        .into_iter()
        .map(|(_, s)| s["name"].clone())
        .collect();
    assert_eq!(names, [json!("llm")]);
}
