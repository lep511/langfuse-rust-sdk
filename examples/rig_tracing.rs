//! Trace a rig agent: its `gen_ai.*` spans reach Langfuse through
//! `otel::tracing_layer`, nested under a Langfuse observation, with model, token
//! usage and messages. Prints the trace URL.
//!
//! Needs an OpenAI-compatible endpoint: `OPENAI_API_KEY`, and `OPENAI_BASE_URL`
//! for other servers (Ollama, vLLM, a proxy, ...); `OPENAI_MODEL` picks the model
//! (default `gpt-4o-mini`).
//!
//! Writes data: one trace, kept so you can open it. Delete it in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... OPENAI_API_KEY=sk-... \
//!   cargo run --example rig_tracing --features tracing-layer
//! ```

use langfuse_rust_sdk::otel::{
    self, ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use rig_agent::prelude::*;
use rig_core::providers::openai::OpenAIConfig;
use tracing::Instrument as _;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    // Export the `tracing` spans of this example too, not only rig's `gen_ai.*` spans.
    let options = TracingOptions::from_env()?.with_should_export_span(|span| {
        otel::is_default_export_span(span) || otel::is_tracing_layer_span(span)
    });
    let tracer = client.tracer(options)?;

    // Only rig's spans and this example's reach the layer.
    let targets = Targets::new()
        .with_target("rig", LevelFilter::INFO)
        .with_target("rig_tracing", LevelFilter::INFO);
    tracing_subscriber::registry()
        .with(otel::tracing_layer(&tracer).with_filter(targets))
        .init();

    let model_name = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let model = OpenAIConfig::from_env()?.client().chat(model_name);
    let agent = AgentBuilder::new(model)
        .name("geography-tutor")
        .preamble("Answer in one short sentence.")
        // rig records prompts and answers only when asked to.
        .record_content_telemetry(true)
        .build();

    let question = "What is the capital of Australia?";
    let request = tracer.start_observation("geography-question", ObservationType::Span);
    request.set_input(question);
    let user = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_session_id("geography-7")
        .with_tags(["rust-sdk", "example", "rig"]);
    let answer = request
        .in_scope(propagate_attributes(user, async {
            // rig-agent starts a separate trace unless a `tracing` span is
            // current: it adopts that span as the agent span. Created in this
            // scope, the span is a child of `request`.
            let span = tracing::info_span!("tutor-agent", gen_ai.operation.name = "invoke_agent");
            async { agent.prompt(question).await }
                .instrument(span)
                .await
        }))
        .await?;
    request.set_output(&answer.output());
    request.end();

    tracer.shutdown().await;
    println!("Answer: {}", answer.output());
    println!("URL: {}", client.trace_url(request.trace_id()).await?);
    Ok(())
}
