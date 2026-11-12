//! `otel::tracing_layer` (feature `tracing-layer`) with rig: the `gen_ai.*`
//! spans of rig agents, tools and model calls against fake providers, checked
//! on the spans exported to a fake Langfuse. In the `integration-tests` member
//! so that the main crate never compiles rig; the plain `tracing` cases are in
//! the main crate's `tests/tracing_layer_tests.rs`.
#![cfg(feature = "tracing-layer")]

mod support;

use std::time::Duration;

use futures::StreamExt;
use langfuse_rust_sdk::otel::{
    self, LangfuseTracer, ObservationType, PropagatedAttributes, TRACING_LAYER_SCOPE,
    TracingOptions, propagate_attributes,
};
use rig_agent::prelude::*;
use rig_core::completion::CompletionRequest;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::{OpenAI, OpenAIConfig};
use rig_core::tool::ToolContext;
use serde_json::{Value, json};
use support::*;
use tracing::Instrument as _;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;
use wiremock::matchers::{body_string_contains, method, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// The model asked for, and the snapshot the fake OpenAI answers with.
const OPENAI_MODEL: &str = "gpt-4o-mini";
const OPENAI_SNAPSHOT: &str = "gpt-4o-mini-2024-07-18";

/// Record the `tracing` spans of this thread on `tracer` until the guard is
/// dropped.
fn subscribe(tracer: &LangfuseTracer) -> tracing::subscriber::DefaultGuard {
    tracing_subscriber::registry()
        .with(otel::tracing_layer(tracer))
        .set_default()
}

/// A chat completion of the fake OpenAI: 12 prompt and 3 completion tokens.
fn chat_completion(message: Value, finish_reason: &str) -> ResponseTemplate {
    json(
        200,
        json!({
            "id": "chatcmpl-1", "object": "chat.completion", "created": 1,
            "model": OPENAI_SNAPSHOT,
            "choices": [{"index": 0, "finish_reason": finish_reason, "message": message}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }),
    )
}

fn answer(text: &str) -> ResponseTemplate {
    chat_completion(json!({"role": "assistant", "content": text}), "stop")
}

/// A fake OpenAI answering every chat completion with `response`.
async fn fake_openai(response: impl Respond + 'static) -> MockServer {
    let openai = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex("chat/completions$"))
        .respond_with(response)
        .mount(&openai)
        .await;
    openai
}

/// A fake OpenAI whose model calls `tool` with `arguments` and, once it has
/// the result, answers `text`.
async fn fake_openai_calling(tool: &str, arguments: &Value, text: &str) -> MockServer {
    let openai = fake_openai(chat_completion(
        json!({"role": "assistant", "content": null, "tool_calls": [{
            "id": "call_1", "type": "function",
            "function": {"name": tool, "arguments": arguments.to_string()}
        }]}),
        "tool_calls",
    ))
    .await;
    // The request that carries the tool result.
    Mock::given(method("POST"))
        .and(path_regex("chat/completions$"))
        .and(body_string_contains(r#""role":"tool""#))
        .respond_with(answer(text))
        .with_priority(1)
        .mount(&openai)
        .await;
    openai
}

/// rig's OpenAI client for the fake `server`.
fn openai(server: &MockServer) -> OpenAI {
    OpenAIConfig::new("sk-test")
        .with_base_url(server.uri())
        .client()
}

/// The bodies of the requests the fake provider received.
async fn provider_requests(server: &MockServer) -> Vec<Value> {
    let requests = server.received_requests().await.unwrap();
    requests.iter().map(body_json).collect()
}

/// A JSON span field (`gen_ai.input.messages`, ...), panicking when absent.
fn field(span: &Span, key: &str) -> Value {
    span.json(key)
        .unwrap_or_else(|| panic!("{} has no {key}", span.name()))
}

#[tokio::test]
async fn rig_agents_become_children_of_the_current_observation() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(answer("Paris")).await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .preamble("Answer with one word.")
        .record_content_telemetry(true)
        .build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    // rig-agent starts a new trace unless a `tracing` span is current: it
    // adopts that span as the agent span. Created inside the observation's
    // scope, the span is the observation's child.
    let response = root
        .in_scope(async {
            let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt("Capital of France?").await }
                .instrument(span)
                .await
        })
        .await
        .unwrap();
    root.end();
    assert_eq!(response.output(), "Paris");

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["answer-question", "chat", "geo-agent"]);
    let root = spans.named("answer-question");
    let agent = spans.named("geo-agent");
    assert_eq!(agent.scope, TRACING_LAYER_SCOPE);
    assert!(agent.is_child_of(root));
    let chat = spans.named("chat");
    assert_eq!(chat.scope, TRACING_LAYER_SCOPE);
    assert!(chat.is_child_of(agent));
    assert_eq!(chat.attr("gen_ai.operation.name"), Some(json!("chat")));
    assert_eq!(chat.attr("gen_ai.provider.name"), Some(json!("openai")));
    assert_eq!(chat.attr("gen_ai.request.model"), Some(json!(OPENAI_MODEL)));
    assert_eq!(
        chat.attr("gen_ai.response.model"),
        Some(json!(OPENAI_SNAPSHOT))
    );
    assert_eq!(chat.attr("gen_ai.request.stream"), Some(json!(false)));
    // `tracing` records unsigned integers through `Debug`, as strings; the
    // exporter turns the token counts back into integers.
    assert_eq!(chat.attr("gen_ai.usage.input_tokens"), Some(json!(12)));
    assert_eq!(chat.attr("gen_ai.usage.output_tokens"), Some(json!(3)));
    assert_eq!(
        field(chat, "gen_ai.input.messages"),
        json!([
            {"role": "system", "parts": [{"type": "text", "content": "Answer with one word."}]},
            {"role": "user", "parts": [{"type": "text", "content": "Capital of France?"}]},
        ])
    );
    assert_eq!(
        field(chat, "gen_ai.output.messages")[0]["parts"],
        json!([{"type": "text", "content": "Paris"}])
    );
    assert_eq!(
        field(chat, "gen_ai.system_instructions"),
        json!([{"type": "text", "content": "Answer with one word."}])
    );
    // No busy/idle timings.
    assert_eq!(chat.attr("busy_ns"), None);
    assert_eq!(chat.attr("idle_ns"), None);
    // The trace has one root for Langfuse: the observation.
    assert_eq!(
        root.attr("langfuse.internal.is_app_root"),
        Some(json!(true))
    );
    assert_eq!(agent.attr("langfuse.internal.is_app_root"), None);
    assert_eq!(chat.attr("langfuse.internal.is_app_root"), None);
}

#[tokio::test]
async fn direct_model_calls_record_model_and_usage() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(answer("Paris")).await;
    let _subscriber = subscribe(&tracer);

    openai(&openai_server)
        .chat(OPENAI_MODEL)
        .call(CompletionRequest::new("Capital of France?"))
        .await
        .unwrap();

    let spans = exported(&tracer, &langfuse).await;
    let chat = spans.named("chat");
    assert_eq!(chat.attr("gen_ai.request.model"), Some(json!(OPENAI_MODEL)));
    assert_eq!(
        chat.attr("gen_ai.usage.output_tokens"),
        Some(json!(3)),
        "an integer"
    );
    // Outside any observation or span, the model call is its own trace.
    assert_eq!(chat.parent_id(), None);
    assert_eq!(
        chat.attr("langfuse.internal.is_app_root"),
        Some(json!(true))
    );
}

#[tokio::test]
async fn agents_outside_a_tracing_span_start_their_own_trace() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(answer("Paris")).await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .name("geo")
        .build();

    // The caveat behind the `tracing` span of the other tests (and of the
    // `rig_tracing` example): without one, rig's `invoke_agent` span is a
    // root, even inside an observation.
    let root = tracer.start_observation("answer-question", ObservationType::Span);
    root.in_scope(async { agent.prompt("Capital of France?").await })
        .await
        .unwrap();
    root.end();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["answer-question", "chat", "invoke_agent"]);
    let root = spans.named("answer-question");
    let agent = spans.named("invoke_agent");
    assert_eq!(agent.parent_id(), None);
    assert_ne!(agent.trace_id(), root.trace_id());
    assert_eq!(agent.attr("gen_ai.agent.name"), Some(json!("geo")));
    assert_eq!(
        agent.attr("langfuse.internal.is_app_root"),
        Some(json!(true))
    );
    // The usage of the run, on the span rig created.
    assert_eq!(agent.attr("gen_ai.usage.input_tokens"), Some(json!(12)));
    assert_eq!(agent.attr("gen_ai.usage.output_tokens"), Some(json!(3)));
    assert!(spans.named("chat").is_child_of(agent));
}

/// A tool that looks the weather up inside a Langfuse observation.
struct Weather {
    tracer: LangfuseTracer,
}

impl Tool for Weather {
    const NAME: &'static str = "weather";
    type Args = Value;
    type Output = String;
    type Error = std::convert::Infallible;

    fn description(&self) -> String {
        "Current weather in a city".into()
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"]
        })
    }

    async fn call(&self, _context: &mut ToolContext, args: Value) -> Result<String, Self::Error> {
        let lookup = self
            .tracer
            .start_observation("lookup", ObservationType::Retriever);
        lookup.set_input(&args);
        let weather = format!("Sunny in {}", args["city"].as_str().unwrap_or("?"));
        lookup.set_output(&weather);
        lookup.end();
        Ok(weather)
    }
}

#[tokio::test]
async fn tool_calls_are_spans_between_the_model_calls() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server =
        fake_openai_calling("weather", &json!({"city": "Lisbon"}), "It is sunny.").await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .name("forecaster")
        .tool(Weather {
            tracer: tracer.clone(),
        })
        .record_content_telemetry(true)
        .build();

    let root = tracer.start_observation("forecast", ObservationType::Agent);
    let response = root
        .in_scope(async {
            let span = tracing::info_span!("forecaster", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt("Weather in Lisbon?").max_turns(3).await }
                .instrument(span)
                .await
        })
        .await
        .unwrap();
    root.end();
    assert_eq!(response.output(), "It is sunny.");

    // The model got the result of the tool.
    let requests = provider_requests(&openai_server).await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]["messages"].as_array().unwrap().last(),
        Some(&json!({"role": "tool", "tool_call_id": "call_1", "content": "Sunny in Lisbon"}))
    );

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans.names(),
        [
            "chat",
            "chat",
            "execute_tool",
            "forecast",
            "forecaster",
            "lookup"
        ]
    );
    let agent = spans.named("forecaster");
    assert!(agent.is_child_of(spans.named("forecast")));
    let chats = spans.all_named("chat");
    let (first, second) = (chats[0], chats[1]);
    let tool = spans.named("execute_tool");
    for span in [first, second, tool] {
        assert!(span.is_child_of(agent), "{}", span.name());
    }
    // One after the other: model call, tool, model call.
    assert!(first.time_range().1 <= tool.time_range().0);
    assert!(tool.time_range().1 <= second.time_range().0);

    assert_eq!(
        field(first, "gen_ai.tool.definitions")[0]["name"],
        "weather"
    );
    assert_eq!(
        field(first, "gen_ai.output.messages")[0]["parts"],
        json!([{
            "type": "tool_call", "id": "call_1", "name": "weather",
            "arguments": {"city": "Lisbon"}
        }])
    );
    assert_eq!(
        tool.attr("gen_ai.operation.name"),
        Some(json!("execute_tool"))
    );
    assert_eq!(tool.attr("gen_ai.tool.name"), Some(json!("weather")));
    assert_eq!(tool.attr("gen_ai.tool.call.id"), Some(json!("call_1")));
    assert_eq!(
        field(tool, "gen_ai.tool.call.arguments"),
        json!({"city": "Lisbon"})
    );
    assert_eq!(
        tool.attr("gen_ai.tool.call.result"),
        Some(json!("Sunny in Lisbon"))
    );
    assert_eq!(
        tool.attr("gen_ai.tool.call.outcome"),
        Some(json!("success"))
    );
    let conversation = field(second, "gen_ai.input.messages");
    assert_eq!(conversation.as_array().map(Vec::len), Some(3));
    assert_eq!(
        field(second, "gen_ai.output.messages")[0]["parts"],
        json!([{"type": "text", "content": "It is sunny."}])
    );

    // Context flows back too: the observation started by the tool is a
    // child of rig's tool span.
    let lookup = spans.named("lookup");
    assert_ne!(lookup.scope, TRACING_LAYER_SCOPE);
    assert!(lookup.is_child_of(tool));
    assert_eq!(
        lookup.attr("langfuse.observation.type"),
        Some(json!("retriever"))
    );
    assert_eq!(
        lookup.json("langfuse.observation.input"),
        Some(json!({"city": "Lisbon"}))
    );
}

/// A streamed chat completion of the fake OpenAI: `deltas`, then the usage.
fn chat_completion_stream(deltas: &[&str]) -> ResponseTemplate {
    let chunk = |choices: Value| {
        json!({
            "id": "chatcmpl-1", "object": "chat.completion.chunk", "created": 1,
            "model": OPENAI_SNAPSHOT, "choices": choices
        })
    };
    let mut chunks: Vec<Value> = deltas
        .iter()
        .map(|text| {
            chunk(json!([{
                "index": 0, "finish_reason": null,
                "delta": {"role": "assistant", "content": text}
            }]))
        })
        .collect();
    chunks.push(chunk(
        json!([{"index": 0, "finish_reason": "stop", "delta": {}}]),
    ));
    let mut usage = chunk(json!([]));
    usage["usage"] = json!({"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15});
    chunks.push(usage);
    let mut body: String = chunks
        .iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect();
    body.push_str("data: [DONE]\n\n");
    ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
}

#[tokio::test]
async fn streamed_runs_record_the_output_and_usage_of_the_stream() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(chat_completion_stream(&["Par", "is"])).await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .record_content_telemetry(true)
        .build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    let output = root
        .in_scope(async {
            let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
            async {
                let mut stream = agent.prompt("Capital of France?").stream();
                let mut output = None;
                while let Some(item) = stream.next().await {
                    if let MultiTurnStreamItem::FinalResponse(response) = item.unwrap() {
                        output = Some(response.output().to_owned());
                    }
                }
                output
            }
            .instrument(span)
            .await
        })
        .await;
    root.end();
    assert_eq!(output.as_deref(), Some("Paris"));
    assert_eq!(
        provider_requests(&openai_server).await[0]["stream"],
        json!(true)
    );

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["answer-question", "chat", "geo-agent"]);
    let agent = spans.named("geo-agent");
    assert!(agent.is_child_of(spans.named("answer-question")));
    let chat = spans.named("chat");
    assert!(chat.is_child_of(agent));
    assert_eq!(chat.attr("gen_ai.request.stream"), Some(json!(true)));
    assert_eq!(
        chat.attr("gen_ai.response.model"),
        Some(json!(OPENAI_SNAPSHOT))
    );
    assert_eq!(chat.attr("gen_ai.usage.input_tokens"), Some(json!(12)));
    assert_eq!(chat.attr("gen_ai.usage.output_tokens"), Some(json!(3)));
    assert_eq!(
        field(chat, "gen_ai.output.messages")[0]["parts"],
        json!([{"type": "text", "content": "Paris"}])
    );
}

#[tokio::test]
async fn content_is_only_recorded_when_rig_is_asked_to() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(answer("Paris")).await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .preamble("Answer with one word.")
        .build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    root.in_scope(async {
        let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
        async {
            agent.prompt("Capital of France?").await.unwrap();
            // Per request, over the default of the agent.
            agent
                .prompt("Capital of France?")
                .record_content_telemetry(true)
                .await
                .unwrap();
        }
        .instrument(span)
        .await
    })
    .await;
    root.end();

    let spans = exported(&tracer, &langfuse).await;
    let chats = spans.all_named("chat");
    let (without, with) = (chats[0], chats[1]);
    for key in [
        "gen_ai.input.messages",
        "gen_ai.output.messages",
        "gen_ai.system_instructions",
    ] {
        assert_eq!(without.attr(key), None, "{key}");
        assert!(with.attr(key).is_some(), "{key}");
    }
    // Model and usage are always there.
    for chat in [without, with] {
        assert_eq!(chat.attr("gen_ai.request.model"), Some(json!(OPENAI_MODEL)));
        assert_eq!(chat.attr("gen_ai.usage.input_tokens"), Some(json!(12)));
    }
}

#[tokio::test]
async fn failed_model_calls_export_their_spans_without_a_response() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(json(
        429,
        json!({"error": {
            "message": "Rate limit reached", "type": "rate_limit_error",
            "code": "rate_limit_exceeded"
        }}),
    ))
    .await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL)).build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    let result = root
        .in_scope(async {
            let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt("Capital of France?").await }
                .instrument(span)
                .await
        })
        .await;
    let error = result.unwrap_err();
    // rig does not mark its spans as failed: the caller records the error.
    root.record_error(&error);
    root.end();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["answer-question", "chat", "geo-agent"]);
    let chat = spans.named("chat");
    assert!(chat.is_child_of(spans.named("geo-agent")));
    assert_eq!(chat.attr("gen_ai.request.model"), Some(json!(OPENAI_MODEL)));
    assert_eq!(chat.attr("gen_ai.response.model"), None);
    assert_eq!(chat.attr("gen_ai.usage.input_tokens"), None);
    assert_eq!(chat.attr("gen_ai.usage.output_tokens"), None);
    let root = spans.named("answer-question");
    assert_eq!(
        root.attr("langfuse.observation.level"),
        Some(json!("ERROR"))
    );
    let message = root.text("langfuse.observation.status_message").unwrap();
    assert!(message.contains("Rate limit reached"), "{message}");
}

#[tokio::test]
async fn anthropic_models_record_cached_tokens_as_integers() {
    const CLAUDE: &str = "claude-sonnet-5-5";
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let anthropic = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex("/v1/messages$"))
        .respond_with(json(
            200,
            json!({
                "id": "msg_1", "type": "message", "role": "assistant", "model": CLAUDE,
                "content": [{"type": "text", "text": "Paris"}],
                "stop_reason": "end_turn", "stop_sequence": null,
                "usage": {
                    "input_tokens": 12, "output_tokens": 3,
                    "cache_read_input_tokens": 2000, "cache_creation_input_tokens": 50
                }
            }),
        ))
        .mount(&anthropic)
        .await;
    let _subscriber = subscribe(&tracer);
    let model = AnthropicConfig::new("sk-ant-test")
        .with_base_url(anthropic.uri())
        .client()
        .completion(CLAUDE);
    let agent = AgentBuilder::new(model)
        .preamble("Answer with one word.")
        .build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    let response = root
        .in_scope(async {
            let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt("Capital of France?").await }
                .instrument(span)
                .await
        })
        .await
        .unwrap();
    root.end();
    assert_eq!(response.output(), "Paris");

    let spans = exported(&tracer, &langfuse).await;
    let chat = spans.named("chat");
    assert!(chat.is_child_of(spans.named("geo-agent")));
    assert_eq!(chat.attr("gen_ai.provider.name"), Some(json!("anthropic")));
    assert_eq!(chat.attr("gen_ai.request.model"), Some(json!(CLAUDE)));
    assert_eq!(chat.attr("gen_ai.response.model"), Some(json!(CLAUDE)));
    // Every `gen_ai.usage.*` count is an integer again, not only the input
    // and output tokens. rig reports the whole input: uncached and cached.
    assert_eq!(chat.attr("gen_ai.usage.input_tokens"), Some(json!(2062)));
    assert_eq!(chat.attr("gen_ai.usage.output_tokens"), Some(json!(3)));
    assert_eq!(
        chat.attr("gen_ai.usage.cache_read.input_tokens"),
        Some(json!(2000))
    );
    assert_eq!(
        chat.attr("gen_ai.usage.cache_creation.input_tokens"),
        Some(json!(50))
    );
}

#[tokio::test]
async fn embeddings_are_spans_under_the_current_observation() {
    const EMBEDDING_MODEL: &str = "text-embedding-3-small";
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex("embeddings$"))
        .respond_with(json(
            200,
            json!({
                "object": "list", "model": EMBEDDING_MODEL,
                "data": [{"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]}],
                "usage": {"prompt_tokens": 4, "total_tokens": 4}
            }),
        ))
        .mount(&openai_server)
        .await;
    let _subscriber = subscribe(&tracer);
    let model = openai(&openai_server).embedding(EMBEDDING_MODEL, None);

    // No `tracing` span here: the current observation is the parent.
    let root = tracer.start_observation("index-document", ObservationType::Span);
    let embedding = root
        .in_scope(model.embed_text("Lisbon is sunny."))
        .await
        .unwrap();
    root.end();
    assert_eq!(embedding.vec.len(), 3);

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.names(), ["embeddings", "index-document"]);
    let embeddings = spans.named("embeddings");
    assert_eq!(embeddings.scope, TRACING_LAYER_SCOPE);
    assert!(embeddings.is_child_of(spans.named("index-document")));
    assert_eq!(
        embeddings.attr("gen_ai.operation.name"),
        Some(json!("embeddings"))
    );
    assert_eq!(
        embeddings.attr("gen_ai.request.model"),
        Some(json!(EMBEDDING_MODEL))
    );
    assert_eq!(embeddings.attr("gen_ai.usage.input_tokens"), Some(json!(4)));
}

#[tokio::test]
async fn propagated_attributes_reach_the_spans_of_rig() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    let openai_server = fake_openai(answer("Paris")).await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL)).build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    let attributes = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_session_id("geography-7")
        .with_tags(["rig"]);
    root.in_scope(propagate_attributes(attributes, async {
        let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
        async { agent.prompt("Capital of France?").await }
            .instrument(span)
            .await
    }))
    .await
    .unwrap();
    root.end();

    // The current observation, and the spans rig started in the scope.
    let spans = exported(&tracer, &langfuse).await;
    for name in ["answer-question", "geo-agent", "chat"] {
        let span = spans.named(name);
        assert_eq!(span.attr("user.id"), Some(json!("user-42")), "{name}");
        assert_eq!(
            span.attr("session.id"),
            Some(json!("geography-7")),
            "{name}"
        );
        assert_eq!(
            span.attr("langfuse.trace.tags"),
            Some(json!(["rig"])),
            "{name}"
        );
    }
}

#[tokio::test]
async fn concurrent_agents_stay_in_their_own_traces() {
    let (tracer, langfuse) = tracer(TracingOptions::default()).await;
    // Answers with the question, late enough for the runs to overlap.
    let openai_server = fake_openai(|request: &Request| {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let question = body["messages"].as_array().unwrap().last().unwrap()["content"]
            .as_str()
            .unwrap()
            .to_owned();
        answer(&format!("answer to {question}")).set_delay(Duration::from_millis(50))
    })
    .await;
    let _subscriber = subscribe(&tracer);
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
        .record_content_telemetry(true)
        .build();

    let runs = (0..4).map(|i| {
        let (tracer, agent) = (&tracer, &agent);
        async move {
            let root = tracer.start_observation(format!("request-{i}"), ObservationType::Span);
            let response = root
                .in_scope(async {
                    let span =
                        tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
                    async { agent.prompt(format!("question {i}")).await }
                        .instrument(span)
                        .await
                })
                .await;
            root.end();
            response.unwrap().output().to_owned()
        }
    });
    let outputs = futures::future::join_all(runs).await;
    assert_eq!(
        outputs,
        [
            "answer to question 0",
            "answer to question 1",
            "answer to question 2",
            "answer to question 3"
        ]
    );

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(spans.len(), 12);
    for i in 0..4 {
        let root = spans.named(&format!("request-{i}"));
        let in_trace: Vec<&Span> = spans
            .iter()
            .filter(|span| span.trace_id() == root.trace_id())
            .collect();
        assert_eq!(in_trace.len(), 3, "request-{i}: {in_trace:#?}");
        let agent = in_trace
            .iter()
            .find(|span| span.name() == "geo-agent")
            .unwrap();
        assert!(agent.is_child_of(root), "request-{i}");
        let chat = in_trace.iter().find(|span| span.name() == "chat").unwrap();
        assert!(chat.is_child_of(agent), "request-{i}");
        assert_eq!(
            field(chat, "gen_ai.output.messages")[0]["parts"][0]["content"],
            json!(format!("answer to question {i}"))
        );
    }
}

#[tokio::test]
async fn a_target_filter_keeps_other_spans_and_events_out() {
    // Every `tracing` span that reaches the layer is exported...
    let options = TracingOptions::default().with_should_export_span(|span| {
        otel::is_default_export_span(span) || otel::is_tracing_layer_span(span)
    });
    let (tracer, langfuse) = tracer(options).await;
    let openai_server = fake_openai(answer("Paris")).await;
    // ... and only those of rig and of this file reach it.
    let targets = Targets::new()
        .with_target("rig", LevelFilter::INFO)
        .with_target(module_path!(), LevelFilter::INFO);
    let _subscriber = tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer).with_filter(targets))
        .set_default();
    let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL)).build();

    let root = tracer.start_observation("answer-question", ObservationType::Span);
    root.in_scope(async {
        let filtered = tracing::info_span!(target: "another_crate", "connection-pool");
        async {
            // No `gen_ai.*` field: exported because of `should_export_span`.
            let span = tracing::info_span!("handle-question", question.id = 7);
            async {
                tracing::debug!("below the level of the filter");
                tracing::info!("asking the model");
                agent.prompt("Capital of France?").await
            }
            .instrument(span)
            .await
        }
        .instrument(filtered)
        .await
    })
    .await
    .unwrap();
    root.end();

    let spans = exported(&tracer, &langfuse).await;
    assert_eq!(
        spans.names(),
        ["answer-question", "chat", "handle-question"]
    );
    // The filtered span leaves no hole in the tree.
    let request = spans.named("handle-question");
    assert!(request.is_child_of(spans.named("answer-question")));
    assert_eq!(request.attr("question.id"), Some(json!(7)));
    assert!(spans.named("chat").is_child_of(request));
    // Events follow the same filter: nothing below INFO, nothing from the
    // HTTP client under rig.
    let events = request.event_names();
    assert!(events.contains(&"asking the model"), "{events:?}");
    assert!(
        !events.contains(&"below the level of the filter"),
        "{events:?}"
    );
    assert_eq!(spans.named("chat").event_names(), Vec::<&str>::new());
}

#[tokio::test]
async fn unsampled_and_disabled_tracers_leave_rig_working() {
    for options in [
        TracingOptions::default().with_sample_rate(0.0),
        TracingOptions::default().with_enabled(false),
    ] {
        let (tracer, langfuse) = tracer(options).await;
        let openai_server = fake_openai(answer("Paris")).await;
        let _subscriber = subscribe(&tracer);
        let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL)).build();

        let root = tracer.start_observation("answer-question", ObservationType::Span);
        let response = root
            .in_scope(async {
                let span = tracing::info_span!("geo-agent", gen_ai.operation.name = "invoke_agent");
                async { agent.prompt("Capital of France?").await }
                    .instrument(span)
                    .await
            })
            .await
            .unwrap();
        root.end();
        assert_eq!(response.output(), "Paris");

        assert!(exported(&tracer, &langfuse).await.is_empty());
    }
}

/// rig and the Bedrock integration in one trace.
#[cfg(feature = "bedrock")]
mod with_bedrock {
    use langfuse_rust_sdk::bedrock::Bedrock;
    use support::bedrock::*;

    use super::*;

    /// A tool that asks a Bedrock model through the traced client.
    struct Translate {
        bedrock: Bedrock,
    }

    impl Tool for Translate {
        const NAME: &'static str = "translate";
        type Args = Value;
        type Output = String;
        type Error = std::convert::Infallible;

        fn description(&self) -> String {
            "Translate a text into Portuguese".into()
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            })
        }

        async fn call(
            &self,
            _context: &mut ToolContext,
            args: Value,
        ) -> Result<String, Self::Error> {
            let text = args["text"].as_str().unwrap_or_default();
            let response = self
                .bedrock
                .converse(|r| r.model_id(MODEL).messages(user(text)))
                .with_name("translation")
                .send()
                .await
                .expect("the fake Bedrock answers");
            let message = response.output().unwrap().as_message().unwrap();
            Ok(message.content[0].as_text().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn bedrock_calls_of_a_rig_tool_are_generations_under_its_span() {
        let (tracer, langfuse) = tracer(TracingOptions::default()).await;
        let openai_server =
            fake_openai_calling("translate", &json!({"text": "Good morning"}), "Bom dia!").await;
        let bedrock_server = MockServer::start().await;
        mock_converse(
            &bedrock_server,
            converse_response(json!([{"text": "Bom dia"}])),
        )
        .await;
        let _subscriber = subscribe(&tracer);
        let agent = AgentBuilder::new(openai(&openai_server).chat(OPENAI_MODEL))
            .tool(Translate {
                bedrock: bedrock(&tracer, &bedrock_server),
            })
            .build();

        let root = tracer.start_observation("translate-greeting", ObservationType::Agent);
        let response = root
            .in_scope(async {
                let span =
                    tracing::info_span!("translator", gen_ai.operation.name = "invoke_agent");
                async {
                    agent
                        .prompt("Say good morning in Portuguese.")
                        .max_turns(3)
                        .await
                }
                .instrument(span)
                .await
            })
            .await
            .unwrap();
        root.end();
        assert_eq!(response.output(), "Bom dia!");

        let spans = exported(&tracer, &langfuse).await;
        assert_eq!(
            spans.names(),
            [
                "chat",
                "chat",
                "execute_tool",
                "translate-greeting",
                "translation",
                "translator"
            ]
        );
        let root = spans.named("translate-greeting");
        assert!(spans.iter().all(|span| span.trace_id() == root.trace_id()));
        // Langfuse observation > rig agent > rig tool > Bedrock generation.
        let agent = spans.named("translator");
        assert!(agent.is_child_of(root));
        let tool = spans.named("execute_tool");
        assert!(tool.is_child_of(agent));
        let generation = spans.named("translation");
        assert!(generation.is_child_of(tool));
        assert_eq!(
            generation.attr("langfuse.observation.type"),
            Some(json!("generation"))
        );
        assert_eq!(
            generation.attr("langfuse.observation.model.name"),
            Some(json!(MODEL))
        );
        assert_eq!(
            generation.json("langfuse.observation.input"),
            Some(json!([{"role": "user", "content": "Good morning"}]))
        );
        assert_eq!(
            generation.json("langfuse.observation.output"),
            Some(json!({"role": "assistant", "content": "Bom dia"}))
        );
        // The result went back to the model of the agent.
        assert_eq!(
            provider_requests(&openai_server).await[1]["messages"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["content"],
            "Bom dia"
        );
    }
}
