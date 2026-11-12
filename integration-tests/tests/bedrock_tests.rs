//! `bedrock::Bedrock` (feature `bedrock`): aws-sdk-bedrockruntime Converse
//! and ConverseStream calls against a fake Bedrock server, checked on the
//! generations exported to a fake Langfuse.
#![cfg(feature = "bedrock")]

mod support;

use std::collections::HashMap;
use std::time::Duration;

use aws_sdk_bedrockruntime::config::retry::RetryConfig;
use aws_sdk_bedrockruntime::config::timeout::TimeoutConfig;
use aws_sdk_bedrockruntime::error::SdkError;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::types::error::ConverseStreamOutputError;
use aws_sdk_bedrockruntime::types::{
    AnyToolChoice, AudioBlock, AudioFormat, AudioSource, AutoToolChoice, CachePointBlock,
    CachePointType, ContentBlock, ContentBlockDelta, ConversationRole, ConverseStreamOutput,
    DocumentBlock, DocumentFormat, DocumentSource, ImageBlock, ImageFormat, ImageSource,
    InferenceConfiguration, ReasoningContentBlock, ReasoningTextBlock, S3Location,
    SpecificToolChoice, SystemContentBlock, Tool, ToolChoice, ToolConfiguration, ToolInputSchema,
    ToolResultBlock, ToolResultContentBlock, ToolResultStatus, ToolSpecification, ToolUseBlock,
    VideoBlock, VideoFormat, VideoSource,
};
use futures::StreamExt;
use langfuse_rust_sdk::bedrock::{Bedrock, TracedConverseStream};
use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
use langfuse_rust_sdk::media::LangfuseMedia;
use langfuse_rust_sdk::models::prompts::{BasePrompt, Prompt};
use langfuse_rust_sdk::otel::{
    LangfuseTracer, ObservationType, PropagatedAttributes, TraceContext, TracingOptions,
    propagate_attributes,
};
use serde_json::{Value, json};
use support::bedrock::*;
use support::*;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request};

/// Name of the generations that are not given one.
const DEFAULT_NAME: &str = "Bedrock-generation";

/// A failed generation: level `ERROR`, a status message that contains
/// `message` and, because a failed call is not billed, zero cost.
#[track_caller]
fn assert_failed(generation: &Span, message: &str) {
    assert_eq!(
        generation.attr("langfuse.observation.level"),
        Some(json!("ERROR"))
    );
    let status = generation
        .text("langfuse.observation.status_message")
        .unwrap();
    assert!(status.contains(message), "{status}");
    assert_eq!(
        generation.json("langfuse.observation.cost_details"),
        Some(json!({"input": 0, "output": 0, "total": 0}))
    );
}

/// A tracer on a fake Langfuse, and a fake Bedrock without routes.
async fn setup() -> (LangfuseTracer, MockServer, MockServer) {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    (tracer, langfuse, MockServer::start().await)
}

fn cache_point() -> CachePointBlock {
    CachePointBlock::builder()
        .r#type(CachePointType::Default)
        .build()
        .unwrap()
}

fn weather_tool() -> Tool {
    Tool::ToolSpec(
        ToolSpecification::builder()
            .name("weather")
            .description("Current weather")
            .input_schema(ToolInputSchema::Json(document(json!({"type": "object"}))))
            .build()
            .unwrap(),
    )
}

fn text_prompt(name: &str, version: i64) -> Prompt {
    serde_json::from_value(json!({
        "type": "text", "name": name, "version": version, "config": {},
        "labels": [], "tags": [], "prompt": "Capital of {{country}}?"
    }))
    .unwrap()
}

// ---------------------------------------------------------------------------
// Converse
// ---------------------------------------------------------------------------

#[tokio::test]
async fn converse_calls_are_generations_under_the_current_observation() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("^/model/[^/]+/converse$"))
        .respond_with(converse_response(json!([{"text": "Paris"}])))
        .expect(1)
        .mount(&server)
        .await;
    let bedrock = bedrock(&tracer, &server);
    let prompt = text_prompt("capitals", 3);

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
                        .additional_model_request_fields(document(json!({"top_k": 40})))
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
    assert_eq!(spans.names(), ["agent", "capital"]);
    let agent = spans.named("agent");
    let generation = spans.named("capital");
    assert!(generation.is_child_of(agent));
    assert_eq!(
        generation.text("langfuse.observation.type").as_deref(),
        Some("generation")
    );
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([
            {"role": "system", "content": "Answer in one word."},
            {"role": "user", "content": "Capital of France?"},
        ]))
    );
    assert_eq!(
        generation
            .text("langfuse.observation.model.name")
            .as_deref(),
        Some(MODEL)
    );
    assert_eq!(
        generation.json("langfuse.observation.model.parameters"),
        Some(json!({
            "max_tokens": 50, "temperature": 0.2, "top_p": 0.9, "stop": ["END"], "top_k": 40,
        }))
    );
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
    assert_eq!(
        generation.json("langfuse.observation.usage_details"),
        Some(json!({
            "input": 12, "output": 3, "total": 2015,
            "input_cache_read": 2000, "input_cache_creation": 0,
        }))
    );
    assert_eq!(generation.metadata("team"), Some(json!("geo")));
    assert_eq!(generation.metadata("stop_reason"), Some(json!("end_turn")));
    assert_eq!(generation.metadata("latency_ms"), Some(json!(321)));
    assert_eq!(
        generation
            .text("langfuse.observation.prompt.name")
            .as_deref(),
        Some("capitals")
    );
    assert_eq!(
        generation.attr("langfuse.observation.prompt.version"),
        Some(json!(3))
    );
    // A successful call has no level, status message or cost of its own.
    assert_eq!(generation.attr("langfuse.observation.level"), None);
    assert_eq!(generation.attr("langfuse.observation.cost_details"), None);
}

#[tokio::test]
async fn the_request_reaches_bedrock_unchanged() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
    let bedrock = bedrock(&tracer, &server);
    // An inference profile ARN: the SDK percent-encodes it into the path.
    let arn =
        "arn:aws:bedrock:us-east-1:123456789012:inference-profile/us.anthropic.claude-sonnet-5-5";

    bedrock
        .converse(|r| {
            r.model_id(arn)
                .system(SystemContentBlock::Text("Answer in one word.".into()))
                .messages(user("Capital of France?"))
                .inference_config(
                    InferenceConfiguration::builder()
                        .max_tokens(50)
                        .stop_sequences("END")
                        .build(),
                )
                .additional_model_request_fields(document(json!({"top_k": 40})))
        })
        .send()
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.path(),
        "/model/arn%3Aaws%3Abedrock%3Aus-east-1%3A123456789012%3Ainference-profile%2Fus.anthropic.claude-sonnet-5-5/converse"
    );
    assert_eq!(
        body_json(&requests[0]),
        json!({
            "system": [{"text": "Answer in one word."}],
            "messages": [{"role": "user", "content": [{"text": "Capital of France?"}]}],
            "inferenceConfig": {"maxTokens": 50, "stopSequences": ["END"]},
            "additionalModelRequestFields": {"top_k": 40},
        })
    );
    // The model is recorded as it was given.
    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans
            .named(DEFAULT_NAME)
            .text("langfuse.observation.model.name")
            .as_deref(),
        Some(arn)
    );
}

#[tokio::test]
async fn requests_without_inference_settings_have_no_model_parameters() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
    let bedrock = bedrock(&tracer, &server);

    bedrock
        .converse(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.attr("langfuse.observation.model.parameters"),
        None
    );
    // Without a current observation the generation starts its own trace.
    assert_eq!(generation.parent_id(), None);
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([{"role": "user", "content": "Capital of France?"}]))
    );
}

#[tokio::test]
async fn tools_and_tool_use_are_recorded() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(
        &server,
        converse_response(json!([
            {"text": "Checking."},
            {"toolUse": {"toolUseId": "tu-1", "name": "weather", "input": {"city": "Lisbon"}}}
        ])),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);
    let tools = ToolConfiguration::builder()
        .tools(weather_tool())
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
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!({
            "tools": [{"toolSpec": {
                "name": "weather", "description": "Current weather",
                "inputSchema": {"json": {"type": "object"}}
            }}],
            "messages": [{"role": "user", "content": "Weather in Lisbon?"}],
        }))
    );
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "text", "text": "Checking."},
            {"type": "tool_use", "id": "tu-1", "name": "weather", "input": {"city": "Lisbon"}},
        ]}))
    );
}

#[tokio::test]
async fn the_tool_choice_and_tool_cache_points_are_recorded() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    let bedrock = bedrock(&tracer, &server);
    let choices = [
        (
            "auto",
            ToolChoice::Auto(AutoToolChoice::builder().build()),
            json!({"auto": {}}),
        ),
        (
            "any",
            ToolChoice::Any(AnyToolChoice::builder().build()),
            json!({"any": {}}),
        ),
        (
            "tool",
            ToolChoice::Tool(
                SpecificToolChoice::builder()
                    .name("weather")
                    .build()
                    .unwrap(),
            ),
            json!({"tool": {"name": "weather"}}),
        ),
    ];

    for (name, choice, _) in &choices {
        let tools = ToolConfiguration::builder()
            .tools(weather_tool())
            .tools(Tool::CachePoint(cache_point()))
            .tool_choice(choice.clone())
            .build()
            .unwrap();
        bedrock
            .converse(|r| r.model_id(MODEL).messages(user("hi")).tool_config(tools))
            .with_name(*name)
            .send()
            .await
            .unwrap();
    }

    let spans = exported(&tracer, &langfuse).await;
    for (name, _, expected) in &choices {
        let input = spans
            .named(name)
            .json("langfuse.observation.input")
            .unwrap();
        assert_eq!(&input["tool_choice"], expected, "{name}");
        assert_eq!(input["tools"][0]["toolSpec"]["name"], "weather", "{name}");
        assert_eq!(
            input["tools"][1],
            json!({"cachePoint": {"type": "default"}}),
            "{name}"
        );
        assert_eq!(
            input["messages"],
            json!([{"role": "user", "content": "hi"}]),
            "{name}"
        );
    }
}

#[tokio::test]
async fn tool_turns_reasoning_and_cache_points_are_chat_parts() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(
        &server,
        converse_response(json!([{"text": "Sunny, 24 °C."}])),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);
    let reasoning = ReasoningContentBlock::ReasoningText(
        ReasoningTextBlock::builder()
            .text("I need the weather tool.")
            .signature("sig")
            .build()
            .unwrap(),
    );
    let tool_use = ToolUseBlock::builder()
        .tool_use_id("tu-1")
        .name("weather")
        .input(document(json!({"city": "Lisbon", "days": 2, "offset": -1})))
        .build()
        .unwrap();
    let tool_result = ToolResultBlock::builder()
        .tool_use_id("tu-1")
        .content(ToolResultContentBlock::Text("Sunny".into()))
        .content(ToolResultContentBlock::Json(document(
            json!({"celsius": 24.5, "humid": false, "alerts": [null]}),
        )))
        .status(ToolResultStatus::Success)
        .build()
        .unwrap();

    bedrock
        .converse(|r| {
            r.model_id(MODEL)
                .system(SystemContentBlock::Text("Be brief.".into()))
                // Only the text blocks of the system prompt are messages.
                .system(SystemContentBlock::CachePoint(cache_point()))
                .messages(user("Weather in Lisbon?"))
                .messages(message(
                    ConversationRole::Assistant,
                    vec![
                        ContentBlock::ReasoningContent(reasoning),
                        ContentBlock::Text("Checking.".into()),
                        ContentBlock::ToolUse(tool_use),
                    ],
                ))
                .messages(message(
                    ConversationRole::User,
                    vec![
                        ContentBlock::ToolResult(tool_result),
                        ContentBlock::CachePoint(cache_point()),
                    ],
                ))
        })
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans.named(DEFAULT_NAME).json("langfuse.observation.input"),
        Some(json!([
            {"role": "system", "content": "Be brief."},
            {"role": "user", "content": "Weather in Lisbon?"},
            {"role": "assistant", "content": [
                {"type": "reasoning", "text": "I need the weather tool."},
                {"type": "text", "text": "Checking."},
                {"type": "tool_use", "id": "tu-1", "name": "weather",
                 "input": {"city": "Lisbon", "days": 2, "offset": -1}},
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "tu-1", "status": "success", "content": [
                    {"type": "text", "text": "Sunny"},
                    {"type": "json", "json": {"celsius": 24.5, "humid": false, "alerts": [null]}},
                ]},
                {"type": "cache_point"},
            ]},
        ]))
    );
}

#[tokio::test]
async fn media_in_the_messages_is_uploaded_and_referenced() {
    // The PNG signature, a tiny PDF and an ID3 tag, with their base64.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const PDF: &[u8] = b"%PDF-1.4 tiny";
    const MP3: &[u8] = b"ID3\x03\x00\x00";
    let png = LangfuseMedia::from_data_uri("data:image/png;base64,iVBORw0KGgo=").unwrap();
    let pdf =
        LangfuseMedia::from_data_uri("data:application/pdf;base64,JVBERi0xLjQgdGlueQ==").unwrap();
    let mp3 = LangfuseMedia::from_data_uri("data:audio/mpeg;base64,SUQzAwAA").unwrap();

    let (tracer, langfuse, server) = setup().await;
    // Already stored: the upload stops after asking for the URL.
    Mock::given(method("POST"))
        .and(path(MEDIA_PATH))
        .respond_with(json(200, json!({"uploadUrl": null, "mediaId": "stored"})))
        .mount(&langfuse)
        .await;
    mock_converse(&server, converse_response(json!([{"text": "A cat."}]))).await;
    let bedrock = bedrock(&tracer, &server);
    let s3 = |uri: &str| S3Location::builder().uri(uri).build().unwrap();
    let content = vec![
        ContentBlock::Text("Compare these.".into()),
        ContentBlock::Image(
            ImageBlock::builder()
                .format(ImageFormat::Png)
                .source(ImageSource::Bytes(Blob::new(PNG)))
                .build()
                .unwrap(),
        ),
        ContentBlock::Image(
            ImageBlock::builder()
                .format(ImageFormat::Jpeg)
                .source(ImageSource::S3Location(s3("s3://bucket/photo.jpg")))
                .build()
                .unwrap(),
        ),
        ContentBlock::Document(
            DocumentBlock::builder()
                .format(DocumentFormat::Pdf)
                .name("report")
                .source(DocumentSource::Bytes(Blob::new(PDF)))
                .build()
                .unwrap(),
        ),
        ContentBlock::Document(
            DocumentBlock::builder()
                .format(DocumentFormat::Txt)
                .name("notes")
                .source(DocumentSource::Text("Lisbon is sunny.".into()))
                .build()
                .unwrap(),
        ),
        ContentBlock::Audio(
            AudioBlock::builder()
                .format(AudioFormat::Mp3)
                .source(AudioSource::Bytes(Blob::new(MP3)))
                .build()
                .unwrap(),
        ),
        ContentBlock::Video(
            VideoBlock::builder()
                .format(VideoFormat::Mp4)
                .source(VideoSource::S3Location(s3("s3://bucket/clip.mp4")))
                .build()
                .unwrap(),
        ),
    ];

    bedrock
        .converse(|r| {
            r.model_id(MODEL)
                .messages(message(ConversationRole::User, content))
        })
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    // Inline bytes are data URIs, which the exporter replaces by references
    // to the uploaded media; S3 locations stay URIs.
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([{"role": "user", "content": [
            {"type": "text", "text": "Compare these."},
            {"type": "image_url", "image_url": {"url": png.reference_string()}},
            {"type": "image_url", "image_url": {"url": "s3://bucket/photo.jpg"}},
            {"type": "document", "name": "report", "format": "pdf",
             "source": pdf.reference_string()},
            {"type": "document", "name": "notes", "format": "txt", "source": "Lisbon is sunny."},
            {"type": "audio", "format": "mp3", "data": mp3.reference_string()},
            {"type": "video", "format": "mp4", "source": "s3://bucket/clip.mp4"},
        ]}]))
    );

    let mut uploads = media_requests(&langfuse).await;
    uploads.sort_by_key(|upload| upload["contentType"].as_str().map(str::to_owned));
    assert_eq!(uploads.len(), 3, "{uploads:#?}");
    for (upload, media, content_type) in [
        (&uploads[0], &pdf, "application/pdf"),
        (&uploads[1], &mp3, "audio/mpeg"),
        (&uploads[2], &png, "image/png"),
    ] {
        assert_eq!(upload["contentType"], content_type);
        assert_eq!(upload["sha256Hash"], media.sha256_hash());
        assert_eq!(upload["field"], "input");
        assert_eq!(upload["traceId"], generation.trace_id());
        assert_eq!(upload["observationId"], generation.id());
    }
    // The request itself carried the bytes, as base64.
    let (_, body) = &bedrock_requests(&server).await[0];
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"image": {"format": "png", "source": {"bytes": "iVBORw0KGgo="}}})
    );
}

#[tokio::test]
async fn reasoning_in_the_response_is_recorded() {
    let (tracer, langfuse, server) = setup().await;
    let mut body = converse_body(json!([
        {"reasoningContent": {"reasoningText": {
            "text": "France's capital is Paris.", "signature": "sig"
        }}},
        {"reasoningContent": {"redactedContent": "cmVkYWN0ZWQ="}},
        {"text": "Paris"},
    ]));
    body["stopReason"] = json!("max_tokens");
    body["usage"] = json!({"inputTokens": 20, "outputTokens": 64, "totalTokens": 84});
    mock_converse(&server, json(200, body)).await;
    let bedrock = bedrock(&tracer, &server);

    bedrock
        .converse(|r| {
            r.model_id(MODEL)
                .messages(user("Capital of France?"))
                .inference_config(InferenceConfiguration::builder().max_tokens(64).build())
                .additional_model_request_fields(document(
                    json!({"thinking": {"type": "enabled", "budget_tokens": 1024}}),
                ))
        })
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "reasoning", "text": "France's capital is Paris."},
            {"type": "reasoning", "redacted": true},
            {"type": "text", "text": "Paris"},
        ]}))
    );
    assert_eq!(
        generation.json("langfuse.observation.model.parameters"),
        Some(json!({
            "max_tokens": 64,
            "thinking": {"type": "enabled", "budget_tokens": 1024},
        }))
    );
    // No cache counters in the response: none are recorded.
    assert_eq!(
        generation.json("langfuse.observation.usage_details"),
        Some(json!({"input": 20, "output": 64, "total": 84}))
    );
    assert_eq!(
        generation.metadata("stop_reason"),
        Some(json!("max_tokens"))
    );
}

#[tokio::test]
async fn a_trace_context_wins_over_the_current_observation() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    let bedrock = bedrock(&tracer, &server);
    let trace_id = create_trace_id(Some("bedrock-call"));
    let parent_id = create_observation_id(Some("remote-parent"));

    let current = tracer.start_observation("unrelated", ObservationType::Span);
    current
        .in_scope(async {
            bedrock
                .converse(|r| r.model_id(MODEL).messages(user("hi")))
                .with_name("root-of-trace")
                .with_trace_context(TraceContext::new(trace_id.clone()))
                .send()
                .await
                .unwrap();
            bedrock
                .converse(|r| r.model_id(MODEL).messages(user("hi")))
                .with_name("under-remote-parent")
                .with_trace_context(TraceContext {
                    trace_id: trace_id.clone(),
                    parent_observation_id: Some(parent_id.clone()),
                })
                .send()
                .await
                .unwrap();
        })
        .await;
    current.end();

    let spans = exported(&tracer, &langfuse).await;
    let unrelated = spans.named("unrelated");
    assert_ne!(unrelated.trace_id(), trace_id);
    let root = spans.named("root-of-trace");
    assert_eq!(root.trace_id(), trace_id);
    assert_ne!(root.parent_id(), Some(unrelated.id()));
    let child = spans.named("under-remote-parent");
    assert_eq!(child.trace_id(), trace_id);
    assert_eq!(child.parent_id(), Some(parent_id.as_str()));
}

#[tokio::test]
async fn spawned_tasks_join_the_trace_through_a_trace_context() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    let bedrock = bedrock(&tracer, &server);

    let parent = tracer.start_observation("fan-out", ObservationType::Agent);
    let context = TraceContext {
        trace_id: parent.trace_id().to_owned(),
        parent_observation_id: Some(parent.id().to_owned()),
    };
    // A spawned task does not inherit the current observation; clones of the
    // wrapper share the client and the tracer.
    let tasks: Vec<_> = (0..3)
        .map(|i| {
            let bedrock = bedrock.clone();
            let context = context.clone();
            tokio::spawn(async move {
                bedrock
                    .converse(|r| r.model_id(MODEL).messages(user("hi")))
                    .with_name(format!("task-{i}"))
                    .with_trace_context(context)
                    .send()
                    .await
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    parent.end();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["fan-out", "task-0", "task-1", "task-2"]);
    let fan_out = spans.named("fan-out");
    for name in ["task-0", "task-1", "task-2"] {
        assert!(spans.named(name).is_child_of(fan_out), "{name}");
    }
}

#[tokio::test]
async fn concurrent_calls_keep_their_own_input_and_output() {
    let (tracer, langfuse, server) = setup().await;
    // Answers with the question, late enough for the calls to overlap.
    mock_converse(&server, |request: &Request| {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let question = body["messages"][0]["content"][0]["text"].as_str().unwrap();
        converse_response(json!([{"text": format!("echo: {question}")}]))
            .set_delay(Duration::from_millis(50))
    })
    .await;
    let bedrock = bedrock(&tracer, &server);

    let parent = tracer.start_observation("agent", ObservationType::Agent);
    let calls = (0..5).map(|i| {
        let bedrock = &bedrock;
        async move {
            let question = format!("question {i}");
            bedrock
                .converse(|r| r.model_id(MODEL).messages(user(&question)))
                .with_name(format!("call-{i}"))
                .send()
                .await
        }
    });
    let responses = parent.in_scope(futures::future::join_all(calls)).await;
    parent.end();
    assert!(responses.iter().all(Result::is_ok), "{responses:?}");

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.len(), 6);
    let agent = spans.named("agent");
    for i in 0..5 {
        let generation = spans.named(&format!("call-{i}"));
        assert!(generation.is_child_of(agent), "call-{i}");
        assert_eq!(
            generation.json("langfuse.observation.input"),
            Some(json!([{"role": "user", "content": format!("question {i}")}]))
        );
        assert_eq!(
            generation.json("langfuse.observation.output"),
            Some(json!({"role": "assistant", "content": format!("echo: question {i}")}))
        );
    }
}

#[tokio::test]
async fn propagated_attributes_reach_the_generation() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let attributes = PropagatedAttributes::default()
        .with_user_id("user-1")
        .with_session_id("session-9")
        .with_tags(["bedrock", "test"]);
    propagate_attributes(attributes, async {
        bedrock
            .converse(|r| r.model_id(MODEL).messages(user("hi")))
            .with_name("call")
            .send()
            .await
            .unwrap();
        let mut stream = bedrock
            .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
            .with_name("stream")
            .send()
            .await
            .unwrap();
        while stream.recv().await.unwrap().is_some() {}
    })
    .await;

    let spans = exported(&tracer, &langfuse).await;
    for name in ["call", "stream"] {
        let generation = spans.named(name);
        assert_eq!(generation.text("user.id").as_deref(), Some("user-1"));
        assert_eq!(generation.text("session.id").as_deref(), Some("session-9"));
        assert_eq!(
            generation.attr("langfuse.trace.tags"),
            Some(json!(["bedrock", "test"]))
        );
    }
}

#[tokio::test]
async fn fallback_prompts_are_not_linked() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    let bedrock = bedrock(&tracer, &server);
    let fallback = Prompt::Text {
        base: BasePrompt {
            name: "capitals".into(),
            is_fallback: true,
            ..Default::default()
        },
        prompt: "Capital of {{country}}?".into(),
    };

    bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .with_prompt(&fallback)
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(generation.attr("langfuse.observation.prompt.name"), None);
    assert_eq!(generation.attr("langfuse.observation.prompt.version"), None);
}

#[tokio::test]
async fn metadata_that_is_not_json_is_left_out() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "ok"}]))).await;
    let bedrock = bedrock(&tracer, &server);
    // JSON objects only have string keys.
    let by_position: HashMap<(u8, u8), &str> = HashMap::from([((1, 2), "x")]);

    bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .with_metadata("grid", &by_position)
        .with_metadata("team", "geo")
        .with_metadata("labels", &["a", "b"])
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(generation.metadata("grid"), None);
    assert_eq!(generation.metadata("team"), Some(json!("geo")));
    assert_eq!(generation.metadata("labels"), Some(json!(["a", "b"])));
}

#[tokio::test]
async fn calls_through_the_wrapped_client_are_not_traced() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
    let bedrock = bedrock(&tracer, &server);

    let response = bedrock
        .client()
        .converse()
        .model_id(MODEL)
        .messages(user("Capital of France?"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.stop_reason().as_str(), "end_turn");

    assert!(exported(&tracer, &langfuse).await.is_empty());
}

#[tokio::test]
async fn unsampled_and_disabled_tracers_still_make_the_calls() {
    for options in [
        TracingOptions::default().with_sample_rate(0.0),
        TracingOptions::default().with_enabled(false),
    ] {
        let (tracer, langfuse) = tracer(options).await;
        let server = MockServer::start().await;
        mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
        mock_converse_stream(&server, event_stream(stream_frames())).await;
        let bedrock = bedrock(&tracer, &server);

        let response = bedrock
            .converse(|r| r.model_id(MODEL).messages(user("Capital of France?")))
            .with_metadata("team", "geo")
            .send()
            .await
            .unwrap();
        assert_eq!(response.usage().unwrap().input_tokens, 12);
        let mut stream = bedrock
            .converse_stream(|r| r.model_id(MODEL).messages(user("Capital of France?")))
            .send()
            .await
            .unwrap();
        assert_eq!(text_of(&mut stream).await, "Paris");
        drop(stream);

        assert_eq!(bedrock_requests(&server).await.len(), 2);
        assert!(exported(&tracer, &langfuse).await.is_empty());
    }
}

#[tokio::test]
async fn failed_calls_are_errors_with_zero_cost() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(
        &server,
        service_error(400, "ValidationException", "Malformed input request"),
    )
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
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "Malformed input request");
    // What was asked is still recorded; there is no output.
    assert_eq!(
        generation
            .text("langfuse.observation.model.name")
            .as_deref(),
        Some(MODEL)
    );
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([{"role": "user", "content": "hi"}]))
    );
    assert_eq!(generation.attr("langfuse.observation.output"), None);
    assert_eq!(generation.attr("langfuse.observation.usage_details"), None);
}

/// A wrapper whose SDK client makes up to `attempts` attempts, a few
/// milliseconds apart.
fn retrying_bedrock(tracer: &LangfuseTracer, server: &MockServer, attempts: u32) -> Bedrock {
    let retries = RetryConfig::standard()
        .with_max_attempts(attempts)
        .with_initial_backoff(Duration::from_millis(1))
        .with_max_backoff(Duration::from_millis(5));
    bedrock_with(tracer, sdk_config(&server.uri()).retry_config(retries))
}

#[tokio::test]
async fn the_retries_of_the_sdk_are_one_generation() {
    let (tracer, langfuse, server) = setup().await;
    Mock::given(method("POST"))
        .and(path_regex("^/model/[^/]+/converse$"))
        .respond_with(service_error(
            429,
            "ThrottlingException",
            "Too many requests",
        ))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
    let bedrock = retrying_bedrock(&tracer, &server, 3);

    bedrock
        .converse(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .send()
        .await
        .unwrap();

    assert_eq!(bedrock_requests(&server).await.len(), 3);
    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.len(), 1, "one generation for the three attempts");
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(generation.attr("langfuse.observation.level"), None);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
}

#[tokio::test]
async fn exhausted_retries_are_one_failed_generation() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(
        &server,
        service_error(429, "ThrottlingException", "Too many requests"),
    )
    .await;
    let bedrock = retrying_bedrock(&tracer, &server, 2);

    let error = bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap_err();
    assert!(
        error
            .as_service_error()
            .is_some_and(|e| e.is_throttling_exception()),
        "{error:?}"
    );

    assert_eq!(bedrock_requests(&server).await.len(), 2);
    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.len(), 1, "one generation for the two attempts");
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "Too many requests");
}

#[tokio::test]
async fn timeouts_are_errors() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(
        &server,
        converse_response(json!([{"text": "late"}])).set_delay(Duration::from_secs(5)),
    )
    .await;
    let timeout = TimeoutConfig::builder()
        .operation_timeout(Duration::from_millis(100))
        .build();
    let bedrock = bedrock_with(&tracer, sdk_config(&server.uri()).timeout_config(timeout));

    let error = bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::TimeoutError(_)), "{error:?}");

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "timed out");
    // The generation lasted as long as the call, not as long as the server.
    let (start, end) = generation.time_range();
    let elapsed = Duration::from_nanos(u64::try_from(end - start).unwrap());
    assert!(
        (Duration::from_millis(100)..Duration::from_secs(4)).contains(&elapsed),
        "{elapsed:?}"
    );
}

#[tokio::test]
async fn connection_failures_are_errors() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    // Reserve a port and close it: the connection is refused.
    let address = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let bedrock = bedrock_with(&tracer, sdk_config(&format!("http://{address}")));

    let error = bedrock
        .converse(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::DispatchFailure(_)), "{error:?}");
    let stream_error = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .with_name("stream")
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(stream_error, SdkError::DispatchFailure(_)),
        "{stream_error:?}"
    );

    let spans = exported(&tracer, &langfuse).await;
    for name in [DEFAULT_NAME, "stream"] {
        let generation = spans.named(name);
        assert_failed(generation, "dispatch failure");
    }
}

#[tokio::test]
async fn clients_built_from_the_shared_aws_config_are_traced() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse(&server, converse_response(json!([{"text": "Paris"}]))).await;
    // Everything the loader would look up (region, credentials, endpoint) is
    // given, so it reads no profile and calls no metadata service.
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-west-2"))
        .credentials_provider(credentials())
        .endpoint_url(server.uri())
        .retry_config(RetryConfig::disabled())
        .load()
        .await;
    let bedrock = Bedrock::new(aws_sdk_bedrockruntime::Client::new(&config), &tracer);

    bedrock
        .converse(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .send()
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans
            .named(DEFAULT_NAME)
            .json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
}

// ---------------------------------------------------------------------------
// ConverseStream
// ---------------------------------------------------------------------------

/// "Paris" in two deltas, then a `map` tool call whose input comes in two
/// deltas: 10 events.
fn stream_frames() -> Vec<Vec<u8>> {
    vec![
        event("messageStart", json!({"role": "assistant"})),
        text_delta(0, "Par"),
        text_delta(0, "is"),
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

/// The output of the generation of a whole [`stream_frames`] stream.
fn streamed_output() -> Value {
    json!({"role": "assistant", "content": [
        {"type": "text", "text": "Paris"},
        {"type": "tool_use", "id": "tu-2", "name": "map", "input": {"city": "Paris"}},
    ]})
}

/// Read the stream to its end and return the text of its deltas.
async fn text_of(stream: &mut TracedConverseStream) -> String {
    let mut text = String::new();
    while let Some(event) = stream.recv().await.unwrap() {
        if let ConverseStreamOutput::ContentBlockDelta(delta) = event
            && let Some(ContentBlockDelta::Text(chunk)) = delta.delta
        {
            text.push_str(&chunk);
        }
    }
    text
}

/// `langfuse.observation.completion_start_time` in nanoseconds since the
/// Unix epoch.
fn completion_start_nanos(generation: &Span) -> Option<u128> {
    let time = generation.json("langfuse.observation.completion_start_time")?;
    let time = chrono::DateTime::parse_from_rfc3339(time.as_str().unwrap()).unwrap();
    Some(u128::try_from(time.timestamp_nanos_opt().unwrap()).unwrap())
}

#[tokio::test]
async fn streams_accumulate_the_output_until_they_end() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| {
            r.model_id(MODEL)
                .messages(user("Capital of France?"))
                .inference_config(InferenceConfiguration::builder().max_tokens(50).build())
        })
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
    // The end of the stream ended the generation: it is exported while the
    // stream is still alive, and reading past the end changes nothing.
    assert!(stream.recv().await.unwrap().is_none());
    let spans = exported(&tracer, &langfuse).await;
    drop(stream);

    let generation = spans.named("streamed");
    assert_eq!(
        generation.text("langfuse.observation.type").as_deref(),
        Some("generation")
    );
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([{"role": "user", "content": "Capital of France?"}]))
    );
    assert_eq!(
        generation
            .text("langfuse.observation.model.name")
            .as_deref(),
        Some(MODEL)
    );
    assert_eq!(
        generation.json("langfuse.observation.model.parameters"),
        Some(json!({"max_tokens": 50}))
    );
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(streamed_output())
    );
    assert_eq!(
        generation.json("langfuse.observation.usage_details"),
        Some(json!({"input": 5, "output": 2, "total": 7}))
    );
    assert_eq!(generation.metadata("stop_reason"), Some(json!("tool_use")));
    assert_eq!(generation.metadata("latency_ms"), Some(json!(100)));
    assert_eq!(generation.attr("langfuse.observation.level"), None);
    let (start, end) = generation.time_range();
    let first_token = completion_start_nanos(generation).unwrap();
    // The attribute has microseconds, the span nanoseconds.
    assert!(
        (start / 1000..=end / 1000).contains(&(first_token / 1000)),
        "{start} <= {first_token} <= {end}"
    );
    // Dropping the finished stream did not end it a second time.
    assert_eq!(exported(&tracer, &langfuse).await.len(), 1);
}

#[tokio::test]
async fn the_request_of_a_stream_reaches_bedrock_unchanged() {
    let (tracer, _langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| {
            r.model_id(MODEL)
                .system(SystemContentBlock::Text("Answer in one word.".into()))
                .messages(user("Capital of France?"))
                .inference_config(InferenceConfiguration::builder().max_tokens(50).build())
        })
        .send()
        .await
        .unwrap();
    assert_eq!(text_of(&mut stream).await, "Paris");

    assert_eq!(
        bedrock_requests(&server).await,
        [(
            format!("/model/{MODEL}/converse-stream"),
            json!({
                "system": [{"text": "Answer in one word."}],
                "messages": [{"role": "user", "content": [{"text": "Capital of France?"}]}],
                "inferenceConfig": {"maxTokens": 50},
            })
        )]
    );
}

#[tokio::test]
async fn into_stream_ends_the_generation_with_the_stream() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let events: Vec<_> = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap()
        .into_stream()
        .collect()
        .await;
    assert_eq!(events.len(), 10);
    assert!(events.iter().all(Result::is_ok), "{events:?}");
    assert!(matches!(
        events.last(),
        Some(Ok(ConverseStreamOutput::Metadata(_)))
    ));

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(streamed_output())
    );
    assert_eq!(
        generation.json("langfuse.observation.usage_details"),
        Some(json!({"input": 5, "output": 2, "total": 7}))
    );
}

#[tokio::test]
async fn reasoning_deltas_and_cached_tokens_are_accumulated() {
    let (tracer, langfuse, server) = setup().await;
    let reasoning = |delta: Value| {
        event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"reasoningContent": delta}}),
        )
    };
    mock_converse_stream(
        &server,
        event_stream(vec![
            event("messageStart", json!({"role": "assistant"})),
            reasoning(json!({"text": "France's capital "})),
            reasoning(json!({"text": "is Paris."})),
            reasoning(json!({"signature": "sig"})),
            event("contentBlockStop", json!({"contentBlockIndex": 0})),
            text_delta(1, "Paris"),
            event("contentBlockStop", json!({"contentBlockIndex": 1})),
            event("messageStop", json!({"stopReason": "end_turn"})),
            event(
                "metadata",
                json!({
                    "usage": {
                        "inputTokens": 4, "outputTokens": 9, "totalTokens": 1113,
                        "cacheReadInputTokens": 1000, "cacheWriteInputTokens": 100
                    },
                    "metrics": {"latencyMs": 250}
                }),
            ),
        ]),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .send()
        .await
        .unwrap();
    assert_eq!(text_of(&mut stream).await, "Paris");

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "reasoning", "text": "France's capital is Paris."},
            {"type": "text", "text": "Paris"},
        ]}))
    );
    assert_eq!(
        generation.json("langfuse.observation.usage_details"),
        Some(json!({
            "input": 4, "output": 9, "total": 1113,
            "input_cache_read": 1000, "input_cache_creation": 100,
        }))
    );
    assert_eq!(generation.metadata("stop_reason"), Some(json!("end_turn")));
}

#[tokio::test]
async fn events_of_unknown_types_are_passed_on_and_not_recorded() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(
        &server,
        event_stream(vec![
            event("messageStart", json!({"role": "assistant"})),
            event(
                "somethingNew",
                json!({"detail": "from a later API version"}),
            ),
            text_delta(0, "Paris"),
            event("messageStop", json!({"stopReason": "end_turn"})),
        ]),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);

    let events: Vec<_> = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap()
        .into_stream()
        .collect()
        .await;
    assert_eq!(events.len(), 4);
    assert!(
        events[1]
            .as_ref()
            .is_ok_and(ConverseStreamOutput::is_unknown),
        "{:?}",
        events[1]
    );

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
    // No metadata event: no usage.
    assert_eq!(generation.attr("langfuse.observation.usage_details"), None);
    assert_eq!(generation.metadata("stop_reason"), Some(json!("end_turn")));
    assert_eq!(generation.metadata("latency_ms"), None);
}

#[tokio::test]
async fn per_call_settings_apply_to_streams_read_after_their_scope() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);
    let prompt = text_prompt("capitals", 7);
    let trace_id = create_trace_id(Some("bedrock-stream"));

    let parent = tracer.start_observation("agent", ObservationType::Agent);
    let (mut scoped, mut remote) = parent
        .in_scope(async {
            let scoped = bedrock
                .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
                .with_name("scoped")
                .with_metadata("team", "geo")
                .with_metadata("attempt", &2)
                .with_prompt(&prompt)
                .send()
                .await
                .unwrap();
            let remote = bedrock
                .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
                .with_name("remote")
                .with_trace_context(TraceContext::new(trace_id.clone()))
                .send()
                .await
                .unwrap();
            (scoped, remote)
        })
        .await;
    // The parent ends before its streams are read.
    parent.end();
    assert_eq!(text_of(&mut scoped).await, "Paris");
    assert_eq!(text_of(&mut remote).await, "Paris");

    let spans = exported(&tracer, &langfuse).await;
    let agent = spans.named("agent");
    let scoped = spans.named("scoped");
    assert!(scoped.is_child_of(agent));
    assert_eq!(scoped.metadata("team"), Some(json!("geo")));
    assert_eq!(scoped.metadata("attempt"), Some(json!(2)));
    // The metadata of the call and of the response are both kept.
    assert_eq!(scoped.metadata("stop_reason"), Some(json!("tool_use")));
    assert_eq!(
        scoped.text("langfuse.observation.prompt.name").as_deref(),
        Some("capitals")
    );
    assert_eq!(
        scoped.attr("langfuse.observation.prompt.version"),
        Some(json!(7))
    );
    assert!(
        scoped.time_range().1 >= agent.time_range().1,
        "the stream ended after its parent"
    );
    let remote = spans.named("remote");
    assert_eq!(remote.trace_id(), trace_id);
    assert_ne!(remote.trace_id(), agent.trace_id());
}

#[tokio::test]
async fn dropped_streams_end_with_what_they_received() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
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
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Par"}))
    );
    assert_eq!(generation.attr("langfuse.observation.usage_details"), None);
    assert_eq!(generation.metadata("stop_reason"), None);
    // Dropping a stream is not an error.
    assert_eq!(generation.attr("langfuse.observation.level"), None);
    assert!(completion_start_nanos(generation).is_some());
}

#[tokio::test]
async fn the_input_of_an_unfinished_tool_call_is_kept_as_text() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    // Up to the first half of the tool input.
    for _ in 0..6 {
        stream.recv().await.unwrap().unwrap();
    }
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans
            .named(DEFAULT_NAME)
            .json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": [
            {"type": "text", "text": "Paris"},
            {"type": "tool_use", "id": "tu-2", "name": "map", "input": "{\"city\":"},
        ]}))
    );
}

#[tokio::test]
async fn streams_dropped_unread_still_end_their_generation() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(&server, event_stream(stream_frames())).await;
    let bedrock = bedrock(&tracer, &server);

    let stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.input"),
        Some(json!([{"role": "user", "content": "hi"}]))
    );
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": []}))
    );
    assert_eq!(completion_start_nanos(generation), None);
}

#[tokio::test]
async fn streams_that_fail_to_start_are_errors_with_zero_cost() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(
        &server,
        service_error(403, "AccessDeniedException", "No access to the model"),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);

    let error = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap_err();
    assert!(
        error
            .as_service_error()
            .is_some_and(|e| e.is_access_denied_exception()),
        "{error:?}"
    );

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "No access to the model");
    assert_eq!(
        generation
            .text("langfuse.observation.model.name")
            .as_deref(),
        Some(MODEL)
    );
    assert_eq!(generation.attr("langfuse.observation.output"), None);
}

#[tokio::test]
async fn errors_in_the_middle_of_a_stream_mark_the_generation() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(
        &server,
        event_stream(vec![
            event("messageStart", json!({"role": "assistant"})),
            text_delta(0, "Par"),
            exception("throttlingException", "Too many tokens"),
        ]),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    let mut received = 0;
    let error = loop {
        match stream.recv().await {
            Ok(Some(_)) => received += 1,
            Ok(None) => panic!("the exception event is an error"),
            Err(error) => break error,
        }
    };
    assert_eq!(received, 2);
    assert!(
        matches!(
            error.as_service_error(),
            Some(ConverseStreamOutputError::ThrottlingException(_))
        ),
        "{error:?}"
    );
    // The error ended the generation, before the stream is dropped.
    let spans = exported(&tracer, &langfuse).await;
    drop(stream);

    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "Too many tokens");
    assert_eq!(exported(&tracer, &langfuse).await.len(), 1);
}

#[tokio::test]
async fn into_stream_stops_after_an_error() {
    let (tracer, langfuse, server) = setup().await;
    mock_converse_stream(
        &server,
        event_stream(vec![
            event("messageStart", json!({"role": "assistant"})),
            exception("modelStreamErrorException", "The model failed"),
            text_delta(0, "never read"),
        ]),
    )
    .await;
    let bedrock = bedrock(&tracer, &server);

    let events: Vec<_> = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap()
        .into_stream()
        .collect()
        .await;
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events[0].is_ok());
    assert!(
        matches!(
            events[1].as_ref().unwrap_err().as_service_error(),
            Some(ConverseStreamOutputError::ModelStreamErrorException(_))
        ),
        "{:?}",
        events[1]
    );

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "The model failed");
}

// --- Streams that arrive over time -----------------------------------------

#[tokio::test]
async fn the_completion_start_time_is_the_arrival_of_the_first_delta() {
    const PAUSE: Duration = Duration::from_millis(150);
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let endpoint = slow_bedrock(vec![
        Step::Frame(event("messageStart", json!({"role": "assistant"}))),
        Step::Pause(PAUSE),
        Step::Frame(text_delta(0, "Par")),
        Step::Pause(PAUSE),
        Step::Frame(text_delta(0, "is")),
        Step::Frame(event("messageStop", json!({"stopReason": "end_turn"}))),
    ])
    .await;
    let bedrock = bedrock_with(&tracer, sdk_config(&endpoint));

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("Capital of France?")))
        .send()
        .await
        .unwrap();
    assert_eq!(text_of(&mut stream).await, "Paris");

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Paris"}))
    );
    let (start, end) = generation.time_range();
    let first_token = completion_start_nanos(generation).unwrap();
    // Not the start of the request: the first delta was sent a pause later
    // (less the microseconds the attribute drops).
    let pause = PAUSE.as_nanos();
    assert!(
        first_token + 1000 >= start + pause,
        "first token {} ms after the start",
        first_token.saturating_sub(start) / 1_000_000
    );
    // Not the end of the stream either, which came another pause later
    // (half of it allows for a late read of the first delta).
    assert!(
        end >= first_token + pause / 2,
        "end {} ms after the first token",
        end.saturating_sub(first_token) / 1_000_000
    );
}

#[tokio::test]
async fn a_connection_lost_in_the_middle_of_a_stream_is_an_error() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let endpoint = slow_bedrock(vec![
        Step::Frame(event("messageStart", json!({"role": "assistant"}))),
        Step::Frame(text_delta(0, "Par")),
        Step::Pause(Duration::from_millis(50)),
        Step::Hangup,
    ])
    .await;
    let bedrock = bedrock_with(&tracer, sdk_config(&endpoint));

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    let mut received = 0;
    let error = loop {
        match stream.recv().await {
            Ok(Some(_)) => received += 1,
            Ok(None) => panic!("a truncated response is not the end of the stream"),
            Err(error) => break error,
        }
    };
    assert_eq!(received, 2);
    assert!(matches!(error, SdkError::DispatchFailure(_)), "{error:?}");

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_failed(generation, "dispatch failure");
}

#[tokio::test]
async fn a_stream_dropped_while_bedrock_is_still_sending_ends_at_once() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let endpoint = slow_bedrock(vec![
        Step::Frame(event("messageStart", json!({"role": "assistant"}))),
        Step::Frame(text_delta(0, "Par")),
        Step::Pause(Duration::from_secs(5)),
        Step::Frame(text_delta(0, "is")),
    ])
    .await;
    let bedrock = bedrock_with(&tracer, sdk_config(&endpoint));

    let mut stream = bedrock
        .converse_stream(|r| r.model_id(MODEL).messages(user("hi")))
        .send()
        .await
        .unwrap();
    stream.recv().await.unwrap().unwrap();
    stream.recv().await.unwrap().unwrap();
    // The caller gives up, e.g. on a timeout of its own.
    let next = tokio::time::timeout(Duration::from_millis(50), stream.recv()).await;
    assert!(next.is_err(), "the next event is seconds away");
    drop(stream);

    let spans = exported(&tracer, &langfuse).await;
    let generation = spans.named(DEFAULT_NAME);
    assert_eq!(
        generation.json("langfuse.observation.output"),
        Some(json!({"role": "assistant", "content": "Par"}))
    );
    let (start, end) = generation.time_range();
    assert!(
        Duration::from_nanos(u64::try_from(end - start).unwrap()) < Duration::from_secs(4),
        "the generation did not wait for the rest of the stream"
    );
}
