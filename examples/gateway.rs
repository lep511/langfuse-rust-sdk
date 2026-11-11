//! Call models through the Langfuse AI Gateway (beta) inside an SDK trace:
//! list the models of the gateway key, then send an Anthropic Messages call
//! with Claude Sonnet 5.5 and a streamed one with Claude Opus 5.5 from an
//! agent observation, so the gateway records its generations as children of
//! that observation. Prints the trace and observation IDs of every recorded
//! generation.
//!
//! The gateway forwards OpenAI and Anthropic API calls with the provider
//! connections of your organization and records each call as a generation
//! with usage and cost, so the SDK does not trace the model calls itself. A
//! call made inside an observation sends its W3C `traceparent` and the
//! propagated attributes (session, tags, metadata, ...) as request headers;
//! that is what puts the gateway's generation into the SDK's trace.
//!
//! **Writes data:** one SDK trace (the agent observation) in the project of
//! the API keys and one gateway generation per model call in the gateway's
//! ingestion project. They are kept so you can open them. Delete the trace in
//! the UI or with `client.traces.delete(trace_id)`. The two model calls are
//! billed by your provider.
//!
//! **Needs:** the `otel` feature, and `LANGFUSE_GATEWAY_KEY` plus
//! `LANGFUSE_GATEWAY_HOST` (or a Langfuse Cloud `LANGFUSE_BASE_URL`, from
//! which the host is derived) besides the usual keys. A gateway key only
//! works on the gateway host of its own region and cannot call the public
//! API. The models are the key's Claude Sonnet 5.5 and Claude Opus 5.5 (US
//! inference profiles first), unless `GATEWAY_ANTHROPIC_MODEL` /
//! `GATEWAY_ANTHROPIC_STREAM_MODEL` name others. An OpenAI Chat Completions
//! call is added only when the OpenAI route offers one of those Claude models
//! or `GATEWAY_OPENAI_MODEL` names a model: the example never picks a model
//! Langfuse has no price for, whose cost would count as $0.
//!
//! If both model lists are empty, or a call fails with
//! `404 No eligible provider connection`, the organization has no enabled
//! provider connection for the key: an admin adds one under AI Gateway →
//! Providers & Models.
//!
//! **Docs:** <https://langfuse.com/docs/gateway/overview>,
//! <https://langfuse.com/docs/gateway/features/tracing>
//!
//! ```sh
//! LANGFUSE_GATEWAY_HOST=https://gateway.cloud.langfuse.com LANGFUSE_GATEWAY_KEY=... \
//!   cargo run --example gateway --features otel
//! ```

use futures::StreamExt;
use langfuse_rust_sdk::gateway::{GatewayClient, GatewayResponseInfo};
use langfuse_rust_sdk::otel::{
    ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::{Value, json};

/// Claude Sonnet 5.5, then Claude Opus 5.5 (US inference profiles first).
const SONNET: &[&str] = &["us.anthropic.claude-sonnet-5-5", "claude-sonnet-5-5"];
const OPUS: &[&str] = &["us.anthropic.claude-opus-5-5", "claude-opus-5-5"];

/// A model ID of `models` (an OpenAI or Anthropic list) containing one of
/// `preferred` (in order), unless the environment variable `env` names one.
/// Gateway model IDs have the form `<connection>:<model>`, e.g.
/// `amazon-bedrock:us.anthropic.claude-sonnet-5-5`, hence `contains`.
fn pick_model(models: &Value, env: &str, preferred: &[&str]) -> Option<String> {
    if let Ok(model) = std::env::var(env) {
        return Some(model);
    }
    let ids: Vec<&str> = models["data"]
        .as_array()?
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    preferred
        .iter()
        .find_map(|needle| ids.iter().find(|id| id.contains(needle)))
        .map(|id| (*id).to_owned())
}

/// What the gateway reports in its `langfuse-*` response headers: the trace
/// and observation ID of the generation it recorded, the provider that
/// served the call and the gateway's request ID (the one to quote in a bug
/// report).
fn print_info(call: &str, info: &GatewayResponseInfo) {
    println!(
        "{call}: trace {} generation {} (provider {}, request {})",
        info.trace_id.as_deref().unwrap_or("-"),
        info.observation_id.as_deref().unwrap_or("-"),
        info.provider.as_deref().unwrap_or("-"),
        info.request_id.as_deref().unwrap_or("-"),
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = client.tracer(TracingOptions::from_env()?)?;
    // A client of its own: the gateway has its own host and key.
    let gateway = GatewayClient::from_env()?;
    println!("gateway: {}", gateway.host());

    // The models the key can call, one list per API format.
    let anthropic_models = gateway.anthropic_models::<Value>().await?.body;
    let openai_models = gateway.openai_models::<Value>().await?.body;
    let sonnet = pick_model(&anthropic_models, "GATEWAY_ANTHROPIC_MODEL", SONNET)
        .ok_or("the gateway key has no Claude Sonnet 5.5; set GATEWAY_ANTHROPIC_MODEL")?;
    let opus = pick_model(&anthropic_models, "GATEWAY_ANTHROPIC_STREAM_MODEL", OPUS)
        .ok_or("the gateway key has no Claude Opus 5.5; set GATEWAY_ANTHROPIC_STREAM_MODEL")?;
    // The gateway does not translate formats: the OpenAI route only serves
    // models of providers with an OpenAI-compatible API.
    let openai_route = pick_model(
        &openai_models,
        "GATEWAY_OPENAI_MODEL",
        &[SONNET, OPUS].concat(),
    );
    println!(
        "{} Anthropic and {} OpenAI models; using {sonnet}, {opus} (stream) and {}",
        anthropic_models["data"].as_array().map_or(0, Vec::len),
        openai_models["data"].as_array().map_or(0, Vec::len),
        openai_route
            .as_deref()
            .unwrap_or("no Claude 5.5 on the OpenAI route: chat completions skipped"),
    );

    // The SDK side of the trace: one agent observation around the calls.
    // The propagated attributes reach the gateway too, as request headers,
    // and end up on its generations.
    let agent = tracer.start_observation("gateway-example", ObservationType::Agent);
    let attributes = PropagatedAttributes::default()
        .with_session_id("rust-sdk-gateway-example")
        .with_tags(["rust-sdk", "gateway-example"])
        .with_metadata("example", "gateway");

    let gateway_project = agent
        .in_scope(propagate_attributes(attributes, async {
            // A plain Anthropic Messages request, with the gateway's model
            // ID. No generation is started here: the gateway records the
            // generation itself, and tracing the call again would duplicate
            // it.
            let question = "In one sentence: why route model calls through a gateway?";
            let reply = gateway
                .messages::<Value, _>(&json!({
                    "model": sonnet,
                    "max_tokens": 200,
                    "messages": [{"role": "user", "content": question}],
                }))
                .await?;
            print_info("messages", &reply.info);
            println!("  {}", reply.body["content"][0]["text"]);
            // The gateway used the trace ID of the `traceparent` header.
            assert_eq!(reply.info.trace_id.as_deref(), Some(agent.trace_id()));

            // Streaming: the headers (and with them the IDs) arrive before
            // the first event; the events are Anthropic's server-sent events.
            let mut stream = gateway
                .messages_stream(&json!({
                    "model": opus,
                    "max_tokens": 100,
                    "messages": [{"role": "user", "content": "Count from 1 to 5."}],
                }))
                .await?;
            print_info("messages_stream", stream.info());
            let mut text = String::new();
            while let Some(event) = stream.next().await {
                let event = event?;
                if event.event.as_deref() == Some("content_block_delta") {
                    let delta: Value = event.json()?;
                    text.push_str(delta["delta"]["text"].as_str().unwrap_or_default());
                }
            }
            println!("  {}", text.trim());

            if let Some(model) = &openai_route {
                let completion = gateway
                    .chat_completions::<Value, _>(&json!({
                        "model": model,
                        "messages": [{"role": "user", "content": "Reply with: pong"}],
                    }))
                    .await?;
                print_info("chat_completions", &completion.info);
                println!("  {}", completion.body["choices"][0]["message"]["content"]);
            }

            // `count_tokens` records no generation. Not every provider
            // counts tokens: with a Bedrock connection the gateway (beta) was
            // seen to answer 200 with Bedrock's error body, hence the check
            // of the response shape.
            match gateway
                .count_tokens::<Value, _>(&json!({
                    "model": sonnet,
                    "messages": [{"role": "user", "content": question}],
                }))
                .await
            {
                Ok(count) if count.body["input_tokens"].is_u64() => {
                    println!("count_tokens: {}", count.body["input_tokens"]);
                }
                Ok(count) => println!("count_tokens unavailable: {}", count.body),
                Err(e) => println!("count_tokens unavailable: {e}"),
            }
            Ok::<_, Box<dyn std::error::Error>>(reply.info.project_id)
        }))
        .await?;
    agent.end();

    // The gateway writes to the ingestion project its admins chose, which is
    // not necessarily the project of the SDK's API keys. The trace ID is the
    // same either way; the trace is complete only when both are one project.
    println!("\nSDK trace: {}", client.trace_url(agent.trace_id()).await?);
    let sdk_project = client.auth_check().await?.id;
    match gateway_project {
        Some(project) if project != sdk_project => println!(
            "note: the gateway records into project {project}, the API keys belong to \
             {sdk_project}: the generations are in the same trace ID but another project. \
             Use API keys of the gateway's ingestion project to see one trace."
        ),
        _ => println!("the gateway's generations are children of the agent span"),
    }
    tracer.shutdown().await;
    Ok(())
}
