//! `openai::OpenAI` (feature `openai`): async-openai calls against a fake
//! OpenAI server, checked on the exported generations.
#![cfg(feature = "openai")]

mod common;

use std::time::Duration;

use async_openai::config::OpenAIConfig;
use async_openai::types::chat::CreateChatCompletionRequest;
use async_openai::types::embeddings::CreateEmbeddingRequest;
use async_openai::types::responses::CreateResponse;
use common::*;
use futures::StreamExt;
use langfuse_rust_sdk::ids::create_trace_id;
use langfuse_rust_sdk::models::prompts::Prompt;
use langfuse_rust_sdk::openai::OpenAI;
use langfuse_rust_sdk::otel::{
    LangfuseTracer, ObservationType, PropagatedAttributes, TraceContext, TracingOptions,
    propagate_attributes,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";

async fn setup() -> (LangfuseTracer, MockServer, MockServer) {
    let (client, langfuse) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&langfuse)
        .await;
    let options = TracingOptions::default().with_flush_interval(Duration::from_secs(3600));
    let tracer = client.tracer(options).unwrap();
    (tracer, langfuse, MockServer::start().await)
}

fn openai(tracer: &LangfuseTracer, server: &MockServer) -> OpenAI {
    let config = OpenAIConfig::new()
        .with_api_base(server.uri())
        .with_api_key("sk-test");
    OpenAI::new(async_openai::Client::with_config(config), tracer)
}

async fn exported(tracer: &LangfuseTracer, server: &MockServer) -> Vec<Value> {
    tracer.flush().await;
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        for resource_spans in body["resourceSpans"].as_array().unwrap() {
            for scope_spans in resource_spans["scopeSpans"].as_array().unwrap() {
                out.extend(scope_spans["spans"].as_array().unwrap().iter().cloned());
            }
        }
    }
    out
}

fn named<'a>(spans: &'a [Value], name: &str) -> &'a Value {
    let found: Vec<&Value> = spans.iter().filter(|s| s["name"] == name).collect();
    assert_eq!(found.len(), 1, "expected one span named {name}: {spans:#?}");
    found[0]
}

fn attr(span: &Value, key: &str) -> Option<String> {
    let kv = span["attributes"]
        .as_array()?
        .iter()
        .find(|kv| kv["key"] == key)?;
    ["stringValue", "intValue"]
        .iter()
        .find_map(|k| kv["value"].get(*k))
        .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
}

fn attr_json(span: &Value, key: &str) -> Option<Value> {
    attr(span, key).map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
}

fn chat_request(body: Value) -> CreateChatCompletionRequest {
    serde_json::from_value(body).unwrap()
}

fn chat_response(message: Value) -> Value {
    json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 1,
        "model": "gpt-4o-mini-2024-07-18", "service_tier": "default",
        "choices": [{"index": 0, "finish_reason": "stop", "message": message}],
        "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15,
                  "prompt_tokens_details": {"cached_tokens": 2}}
    })
}

async fn mock_post(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn sse(events: &[Value], done: bool) -> ResponseTemplate {
    let mut body = String::new();
    for event in events {
        body.push_str(&format!("data: {event}\n\n"));
    }
    if done {
        body.push_str("data: [DONE]\n\n");
    }
    ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
}

#[tokio::test]
async fn chat_completions_are_generations_under_the_current_observation() {
    let (tracer, langfuse, server) = setup().await;
    mock_post(
        &server,
        "/chat/completions",
        json(
            200,
            chat_response(json!({"role": "assistant", "content": "Paris"})),
        ),
    )
    .await;
    let openai = openai(&tracer, &server);
    let tools = json!([{"type": "function", "function": {"name": "lookup", "parameters": {}}}]);
    let request = chat_request(json!({
        "model": "gpt-4o-mini", "temperature": 0.2, "max_completion_tokens": 50, "seed": 7,
        "messages": [{"role": "system", "content": "Be brief"},
                     {"role": "user", "content": "Capital of France?"}],
        "tools": tools,
        "response_format": {"type": "text"}
    }));

    let root = tracer.start_observation("root", ObservationType::Span);
    root.in_scope(async {
        let response = openai.chat().create(request).await.unwrap();
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Paris")
        );
        // The generation is not made current: this span is its sibling.
        tracer
            .start_observation("after", ObservationType::Span)
            .end();
    })
    .await;
    root.end();

    let spans = exported(&tracer, &langfuse).await;
    let root = named(&spans, "root");
    let generation = named(&spans, "OpenAI-generation");
    assert_eq!(generation["parentSpanId"], root["spanId"]);
    assert_eq!(named(&spans, "after")["parentSpanId"], root["spanId"]);
    assert_eq!(
        attr(generation, "langfuse.observation.type").as_deref(),
        Some("generation")
    );
    assert_eq!(
        attr(generation, "langfuse.observation.model.name").as_deref(),
        Some("gpt-4o-mini-2024-07-18")
    );
    let input = attr_json(generation, "langfuse.observation.input").unwrap();
    assert_eq!(input["tools"][0]["function"]["name"], "lookup");
    assert_eq!(input["messages"][1]["content"], "Capital of France?");
    assert_eq!(
        attr_json(generation, "langfuse.observation.model.parameters"),
        Some(
            json!({"temperature": 0.2, "max_completion_tokens": 50, "top_p": 1,
                    "frequency_penalty": 0, "presence_penalty": 0, "seed": 7,
                    "service_tier": "default"})
        )
    );
    let output = attr_json(generation, "langfuse.observation.output").unwrap();
    assert_eq!(output["role"], "assistant");
    assert_eq!(output["content"], "Paris");
    assert_eq!(
        attr_json(generation, "langfuse.observation.usage_details"),
        Some(
            json!({"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15,
                    "prompt_tokens_details": {"cached_tokens": 2}})
        )
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.response_format"),
        Some(json!({"type": "text"}))
    );
    assert_eq!(
        attr(generation, "langfuse.observation.completion_start_time"),
        None
    );
}

#[tokio::test]
async fn per_call_name_metadata_prompt_and_trace() {
    let (tracer, langfuse, server) = setup().await;
    mock_post(
        &server,
        "/chat/completions",
        json(
            200,
            chat_response(json!({"role": "assistant", "content": "ok"})),
        ),
    )
    .await;
    let openai = openai(&tracer, &server);
    let prompt: Prompt = serde_json::from_value(json!({
        "type": "text", "name": "support", "version": 4, "config": {},
        "labels": [], "tags": [], "prompt": "Hi"
    }))
    .unwrap();
    let trace_id = create_trace_id(Some("openai-call"));
    let user = PropagatedAttributes::default().with_user_id("user-1");
    propagate_attributes(user, async {
        openai
            .chat()
            .with_name("support-answer")
            .with_metadata("team", "search")
            .with_prompt(&prompt)
            .with_trace_context(TraceContext::new(trace_id.clone()))
            .create(chat_request(json!({
                "model": "gpt-4o", "messages": [{"role": "user", "content": "Hi"}]
            })))
            .await
            .unwrap();
    })
    .await;

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "support-answer");
    assert_eq!(generation["traceId"], json!(trace_id));
    assert_eq!(attr(generation, "user.id").as_deref(), Some("user-1"));
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.team"),
        Some(json!("search"))
    );
    assert_eq!(
        attr(generation, "langfuse.observation.prompt.name").as_deref(),
        Some("support")
    );
    assert_eq!(
        attr(generation, "langfuse.observation.prompt.version").as_deref(),
        Some("4")
    );
    // Model parameters default like in the Python SDK.
    assert_eq!(
        attr_json(generation, "langfuse.observation.model.parameters"),
        Some(
            json!({"temperature": 1, "max_tokens": "Infinity", "top_p": 1,
                    "frequency_penalty": 0, "presence_penalty": 0, "service_tier": "default"})
        )
    );
}

#[tokio::test]
async fn streamed_chat_completions_accumulate_chunks() {
    let (tracer, langfuse, server) = setup().await;
    let chunk = |delta: Value, finish: Value| {
        json!({"id": "c", "object": "chat.completion.chunk", "created": 1,
               "model": "gpt-4o-mini-2024-07-18",
               "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
    };
    let mut usage = chunk(json!({}), Value::Null);
    usage["choices"] = json!([]);
    usage["usage"] = json!({"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10});
    mock_post(
        &server,
        "/chat/completions",
        sse(
            &[
                chunk(json!({"role": "assistant", "content": "Hel"}), Value::Null),
                chunk(json!({"content": "lo"}), json!("stop")),
                usage,
            ],
            true,
        ),
    )
    .await;
    let openai = openai(&tracer, &server);
    let mut stream = openai
        .chat()
        .create_stream(chat_request(json!({
            "model": "gpt-4o-mini", "messages": [{"role": "user", "content": "Hi"}],
            "stream_options": {"include_usage": true}
        })))
        .await
        .unwrap();
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        for choice in chunk.unwrap().choices {
            text.push_str(choice.delta.content.as_deref().unwrap_or_default());
        }
    }
    assert_eq!(text, "Hello");
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "OpenAI-generation");
    assert_eq!(
        attr_json(generation, "langfuse.observation.output"),
        Some(json!("Hello"))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.finish_reason"),
        Some(json!("stop"))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.usage_details"),
        Some(json!({"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10}))
    );
    assert!(attr(generation, "langfuse.observation.completion_start_time").is_some());
    assert_eq!(
        attr(generation, "langfuse.observation.model.name").as_deref(),
        Some("gpt-4o-mini-2024-07-18")
    );
}

#[tokio::test]
async fn errors_are_recorded_and_returned() {
    let (tracer, langfuse, server) = setup().await;
    mock_post(
        &server,
        "/chat/completions",
        json(
            400,
            json!({"error": {"message": "Invalid model", "type": "invalid_request_error",
                             "param": "model", "code": "model_not_found"}}),
        ),
    )
    .await;
    let openai = openai(&tracer, &server);
    let error = openai
        .chat()
        .create(chat_request(json!({
            "model": "gpt-unknown", "messages": [{"role": "user", "content": "Hi"}]
        })))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Invalid model"), "{error}");

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "OpenAI-generation");
    assert_eq!(
        attr(generation, "langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert!(
        attr(generation, "langfuse.observation.status_message")
            .unwrap()
            .contains("Invalid model")
    );
    assert_eq!(
        attr(generation, "langfuse.observation.model.name").as_deref(),
        Some("gpt-unknown")
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.cost_details"),
        Some(json!({"input": 0, "output": 0, "total": 0}))
    );
}

#[tokio::test]
async fn embeddings_are_embedding_observations() {
    let (tracer, langfuse, server) = setup().await;
    mock_post(
        &server,
        "/embeddings",
        json(
            200,
            json!({"object": "list", "model": "text-embedding-3-small",
                   "data": [{"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]},
                            {"object": "embedding", "index": 1, "embedding": [0.4, 0.5, 0.6]}],
                   "usage": {"prompt_tokens": 6, "total_tokens": 6}}),
        ),
    )
    .await;
    let openai = openai(&tracer, &server);
    let request: CreateEmbeddingRequest = serde_json::from_value(json!({
        "model": "text-embedding-3-small", "input": ["a", "b"], "dimensions": 3
    }))
    .unwrap();
    openai.embeddings().create(request).await.unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let embedding = named(&spans, "OpenAI-embedding");
    assert_eq!(
        attr(embedding, "langfuse.observation.type").as_deref(),
        Some("embedding")
    );
    assert_eq!(
        attr_json(embedding, "langfuse.observation.input"),
        Some(json!(["a", "b"]))
    );
    assert_eq!(
        attr_json(embedding, "langfuse.observation.output"),
        Some(json!({"dimensions": 3, "count": 2}))
    );
    assert_eq!(
        attr_json(embedding, "langfuse.observation.usage_details"),
        Some(json!({"input": 6}))
    );
    assert_eq!(
        attr_json(embedding, "langfuse.observation.model.parameters"),
        Some(json!({"dimensions": 3}))
    );
}

fn response_json(text: &str) -> Value {
    json!({
        "id": "resp_1", "object": "response", "created_at": 1, "status": "completed",
        "model": "gpt-4.1-2025-04-14",
        "output": [{"type": "message", "id": "msg_1", "status": "completed", "role": "assistant",
                    "content": [{"type": "output_text", "text": text, "annotations": []}]}],
        "usage": {"input_tokens": 9, "output_tokens": 2, "total_tokens": 11,
                  "input_tokens_details": {"cached_tokens": 0},
                  "output_tokens_details": {"reasoning_tokens": 0}}
    })
}

#[tokio::test]
async fn responses_api_calls_and_streams() {
    let (tracer, langfuse, server) = setup().await;
    mock_post(&server, "/responses", json(200, response_json("Hi there"))).await;
    let openai = openai(&tracer, &server);
    let request: CreateResponse = serde_json::from_value(json!({
        "model": "gpt-4.1", "instructions": "Be brief", "input": "Hi",
        "reasoning": {"effort": "low"}, "max_output_tokens": 100
    }))
    .unwrap();
    openai
        .responses()
        .with_name("responses-call")
        .create(request.clone())
        .await
        .unwrap();

    server.reset().await;
    let mut created = response_json("");
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created.as_object_mut().unwrap().remove("usage");
    mock_post(
        &server,
        "/responses",
        sse(
            &[
                json!({"type": "response.created", "sequence_number": 0, "response": created}),
                json!({"type": "response.output_text.delta", "sequence_number": 1,
                       "item_id": "msg_1", "output_index": 0, "content_index": 0,
                       "delta": "Hi there", "logprobs": []}),
                json!({"type": "response.completed", "sequence_number": 2,
                       "response": response_json("Hi there")}),
            ],
            false,
        ),
    )
    .await;
    let mut stream = openai
        .responses()
        .with_name("responses-stream")
        .create_stream(request)
        .await
        .unwrap();
    let mut events = 0;
    while let Some(event) = stream.next().await {
        event.unwrap();
        events += 1;
    }
    assert_eq!(events, 3);
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    for name in ["responses-call", "responses-stream"] {
        let generation = named(&spans, name);
        assert_eq!(
            attr_json(generation, "langfuse.observation.input"),
            Some(json!([{"role": "system", "content": "Be brief"},
                        {"role": "user", "content": "Hi"}])),
            "{name}"
        );
        let output = attr_json(generation, "langfuse.observation.output").unwrap();
        assert_eq!(output["content"][0]["text"], "Hi there", "{name}");
        assert_eq!(
            attr_json(generation, "langfuse.observation.usage_details").unwrap()["total_tokens"],
            11
        );
        let parameters = attr_json(generation, "langfuse.observation.model.parameters").unwrap();
        assert_eq!(parameters["reasoning_effort"], "low");
        assert_eq!(parameters["max_output_tokens"], 100);
        assert!(parameters.get("max_tokens").is_none());
        assert_eq!(
            attr(generation, "langfuse.observation.model.name").as_deref(),
            Some("gpt-4.1-2025-04-14")
        );
    }
    let streamed = named(&spans, "responses-stream");
    assert_eq!(
        attr_json(streamed, "langfuse.observation.metadata.id"),
        Some(json!("resp_1"))
    );
    assert!(attr(streamed, "langfuse.observation.completion_start_time").is_some());
}
