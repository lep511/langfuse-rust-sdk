//! `bedrock::Bedrock` (feature `bedrock`): aws-sdk-bedrockruntime Converse
//! calls against a fake Bedrock server, checked on the exported generations.
#![cfg(feature = "bedrock")]

mod common;

use std::time::Duration;

use aws_sdk_bedrockruntime::config::retry::RetryConfig;
use aws_sdk_bedrockruntime::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ContentBlockDelta, ConversationRole, ConverseStreamOutput,
    InferenceConfiguration, Message, SystemContentBlock, Tool, ToolConfiguration, ToolInputSchema,
    ToolSpecification,
};
use aws_smithy_types::Document;
use aws_smithy_types::event_stream::{Header, HeaderValue, Message as EventMessage};
use common::*;
use langfuse_rust_sdk::bedrock::Bedrock;
use langfuse_rust_sdk::models::prompts::Prompt;
use langfuse_rust_sdk::otel::{LangfuseTracer, ObservationType, TracingOptions};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";
const MODEL: &str = "us.anthropic.claude-opus-5-5";

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

fn bedrock(tracer: &LangfuseTracer, server: &MockServer) -> Bedrock {
    let config = aws_sdk_bedrockruntime::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("AKIDTEST", "secret", None, None, "test"))
        .endpoint_url(server.uri())
        .retry_config(RetryConfig::disabled())
        .build();
    Bedrock::new(aws_sdk_bedrockruntime::Client::from_conf(config), tracer)
}

async fn exported(tracer: &LangfuseTracer, server: &MockServer) -> Vec<Value> {
    tracer.flush().await;
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body: Value = body_json(&request);
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

fn user(text: &str) -> Message {
    Message::builder()
        .role(ConversationRole::User)
        .content(ContentBlock::Text(text.into()))
        .build()
        .unwrap()
}

fn converse_response(content: Value) -> ResponseTemplate {
    json(
        200,
        json!({
            "output": {"message": {"role": "assistant", "content": content}},
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 12, "outputTokens": 3, "totalTokens": 2015,
                "cacheReadInputTokens": 2000, "cacheWriteInputTokens": 0
            },
            "metrics": {"latencyMs": 321}
        }),
    )
}

#[tokio::test]
async fn converse_calls_are_generations_under_the_current_observation() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("/model/.+/converse$"))
        .respond_with(converse_response(json!([{"text": "Paris"}])))
        .expect(1)
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);
    let prompt: Prompt = serde_json::from_value(json!({
        "type": "text", "name": "capitals", "version": 3, "config": {},
        "labels": [], "tags": [], "prompt": "Capital of {{country}}?"
    }))
    .unwrap();

    let parent = tracer.start_observation("agent", ObservationType::Agent);
    let response = parent
        .in_scope(async {
            bedrock
                .converse(|r| {
                    r.model_id(MODEL)
                        .system(SystemContentBlock::Text("Answer in one word.".into()))
                        .messages(user("Capital of France?"))
                        .inference_config(
                            InferenceConfiguration::builder()
                                .max_tokens(50)
                                .temperature(0.2)
                                .top_p(0.9)
                                .stop_sequences("END")
                                .build(),
                        )
                        .additional_model_request_fields(Document::Object(
                            [("top_k".to_owned(), Document::from(40_u64))]
                                .into_iter()
                                .collect(),
                        ))
                })
                .with_name("capital")
                .with_metadata("team", "geo")
                .with_prompt(&prompt)
                .send()
                .await
        })
        .await
        .unwrap();
    parent.end();
    assert_eq!(
        response.output().unwrap().as_message().unwrap().content[0]
            .as_text()
            .unwrap(),
        "Paris"
    );

    let spans = exported(&tracer, &langfuse).await;
    let agent = named(&spans, "agent");
    let generation = named(&spans, "capital");
    assert_eq!(generation["parentSpanId"], agent["spanId"]);
    assert_eq!(
        attr(generation, "langfuse.observation.type").as_deref(),
        Some("generation")
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.input"),
        Some(json!([
            {"role": "system", "content": "Answer in one word."},
            {"role": "user", "content": "Capital of France?"},
        ]))
    );
    assert_eq!(
        attr(generation, "langfuse.observation.model.name").as_deref(),
        Some(MODEL)
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.model.parameters"),
        Some(json!({
            "max_tokens": 50, "temperature": 0.2, "top_p": 0.9, "stop": ["END"], "top_k": 40,
        }))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.usage_details"),
        Some(json!({
            "input": 12, "output": 3, "total": 2015,
            "input_cache_read": 2000, "input_cache_creation": 0,
        }))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.team"),
        Some(json!("geo"))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.stop_reason"),
        Some(json!("end_turn"))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.latency_ms"),
        Some(json!(321))
    );
    assert_eq!(
        attr(generation, "langfuse.observation.prompt.name").as_deref(),
        Some("capitals")
    );
}

#[tokio::test]
async fn tools_and_tool_use_are_recorded() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("/converse$"))
        .respond_with(converse_response(json!([
            {"text": "Checking."},
            {"toolUse": {"toolUseId": "tu-1", "name": "weather", "input": {"city": "Lisbon"}}}
        ])))
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);
    let schema = Document::Object(
        [("type".to_owned(), Document::String("object".into()))]
            .into_iter()
            .collect(),
    );
    let tools = ToolConfiguration::builder()
        .tools(Tool::ToolSpec(
            ToolSpecification::builder()
                .name("weather")
                .description("Current weather")
                .input_schema(ToolInputSchema::Json(schema))
                .build()
                .unwrap(),
        ))
        .build()
        .unwrap();

    bedrock
        .converse(|r| {
            r.model_id(MODEL)
                .messages(user("Weather in Lisbon?"))
                .tool_config(tools)
        })
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "Bedrock-generation");
    assert_eq!(
        attr_json(generation, "langfuse.observation.input"),
        Some(json!({
            "tools": [{"toolSpec": {
                "name": "weather", "description": "Current weather",
                "inputSchema": {"json": {"type": "object"}}
            }}],
            "messages": [{"role": "user", "content": "Weather in Lisbon?"}],
        }))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "text", "text": "Checking."},
            {"type": "tool_use", "id": "tu-1", "name": "weather", "input": {"city": "Lisbon"}},
        ]}))
    );
}

#[tokio::test]
async fn failed_calls_are_errors_with_zero_cost() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("/converse$"))
        .respond_with(
            json(400, json!({"message": "Malformed input request"}))
                .insert_header("x-amzn-ErrorType", "ValidationException"),
        )
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);

    let error = bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap_err();
    assert!(
        error
            .as_service_error()
            .is_some_and(|e| e.is_validation_exception()),
        "{error:?}"
    );

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "Bedrock-generation");
    assert_eq!(
        attr(generation, "langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    let message = attr(generation, "langfuse.observation.status_message").unwrap();
    assert!(message.contains("Malformed input request"), "{message}");
    assert_eq!(
        attr_json(generation, "langfuse.observation.cost_details"),
        Some(json!({"input": 0, "output": 0, "total": 0}))
    );
    assert_eq!(
        attr(generation, "langfuse.observation.model.name").as_deref(),
        Some(MODEL)
    );
}

/// One event-stream frame of a ConverseStream response.
fn event(event_type: &'static str, payload: Value) -> Vec<u8> {
    let message = EventMessage::new(payload.to_string().into_bytes())
        .add_header(Header::new(
            ":message-type",
            HeaderValue::String("event".into()),
        ))
        .add_header(Header::new(
            ":event-type",
            HeaderValue::String(event_type.into()),
        ))
        .add_header(Header::new(
            ":content-type",
            HeaderValue::String("application/json".into()),
        ));
    let mut frame = Vec::new();
    aws_smithy_eventstream::frame::write_message_to(&message, &mut frame).unwrap();
    frame
}

fn event_stream(frames: Vec<Vec<u8>>) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(frames.concat(), "application/vnd.amazon.eventstream")
}

fn stream_frames() -> Vec<Vec<u8>> {
    vec![
        event("messageStart", json!({"role": "assistant"})),
        event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"text": "Par"}}),
        ),
        event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"text": "is"}}),
        ),
        event("contentBlockStop", json!({"contentBlockIndex": 0})),
        event(
            "contentBlockStart",
            json!({"contentBlockIndex": 1, "start": {"toolUse": {"toolUseId": "tu-2", "name": "map"}}}),
        ),
        event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 1, "delta": {"toolUse": {"input": "{\"city\":"}}}),
        ),
        event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 1, "delta": {"toolUse": {"input": "\"Paris\"}"}}}),
        ),
        event("contentBlockStop", json!({"contentBlockIndex": 1})),
        event("messageStop", json!({"stopReason": "tool_use"})),
        event(
            "metadata",
            json!({
                "usage": {"inputTokens": 5, "outputTokens": 2, "totalTokens": 7},
                "metrics": {"latencyMs": 100}
            }),
        ),
    ]
}

#[tokio::test]
async fn streams_accumulate_the_output_until_they_end() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("/model/.+/converse-stream$"))
        .respond_with(event_stream(stream_frames()))
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .with_name("streamed")
        .send()
        .await
        .unwrap();
    let mut text = String::new();
    let mut events = 0;
    while let Some(event) = stream.recv().await.unwrap() {
        events += 1;
        if let ConverseStreamOutput::ContentBlockDelta(delta) = event
            && let Some(ContentBlockDelta::Text(chunk)) = delta.delta
        {
            text.push_str(&chunk);
        }
    }
    assert_eq!(text, "Paris");
    assert_eq!(events, 10);
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "streamed");
    assert_eq!(
        attr_json(generation, "langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "text", "text": "Paris"},
            {"type": "tool_use", "id": "tu-2", "name": "map", "input": {"city": "Paris"}},
        ]}))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.usage_details"),
        Some(json!({"input": 5, "output": 2, "total": 7}))
    );
    assert_eq!(
        attr_json(generation, "langfuse.observation.metadata.stop_reason"),
        Some(json!("tool_use"))
    );
    assert!(attr(generation, "langfuse.observation.completion_start_time").is_some());
}

#[tokio::test]
async fn dropped_streams_end_with_what_they_received() {
    use futures::StreamExt;

    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("/converse-stream$"))
        .respond_with(event_stream(stream_frames()))
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);

    let stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap()
        .into_stream();
    // Read up to the first text delta, then drop the stream.
    let first: Vec<_> = stream.take(2).collect().await;
    assert_eq!(first.len(), 2);

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "Bedrock-generation");
    assert_eq!(
        attr_json(generation, "langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Par"}))
    );
    assert!(attr(generation, "langfuse.observation.usage_details").is_none());
}

#[tokio::test]
async fn errors_in_the_middle_of_a_stream_mark_the_generation() {
    let (tracer, langfuse, server) = setup().await;
    let exception = EventMessage::new(
        json!({"message": "Too many tokens"})
            .to_string()
            .into_bytes(),
    )
    .add_header(Header::new(
        ":message-type",
        HeaderValue::String("exception".into()),
    ))
    .add_header(Header::new(
        ":exception-type",
        HeaderValue::String("throttlingException".into()),
    ))
    .add_header(Header::new(
        ":content-type",
        HeaderValue::String("application/json".into()),
    ));
    let mut exception_frame = Vec::new();
    aws_smithy_eventstream::frame::write_message_to(&exception, &mut exception_frame).unwrap();
    Mock::given(method("POST"))
        .and(path_regex("/converse-stream$"))
        .respond_with(event_stream(vec![
            event("messageStart", json!({"role": "assistant"})),
            event(
                "contentBlockDelta",
                json!({"contentBlockIndex": 0, "delta": {"text": "Par"}}),
            ),
            exception_frame,
        ]))
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    let mut failed = None;
    loop {
        match stream.recv().await {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(error) => {
                failed = Some(error);
                break;
            }
        }
    }
    assert!(failed.is_some(), "the exception event is an error");
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    let generation = named(&spans, "Bedrock-generation");
    assert_eq!(
        attr(generation, "langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    let message = attr(generation, "langfuse.observation.status_message").unwrap();
    assert!(message.contains("Too many tokens"), "{message}");
}
