//! Trace one chat request with the SDK's tracer: an agent observation with a
//! retriever, a generation and an event nested under it, user, session, tags
//! and trace name propagated to all of them, and a score on the generation.
//! Prints the trace URL.
//!
//! **Writes data:** one trace (four observations) and one score, kept so you
//! can open them. Delete the trace in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! **Needs:** the `otel` feature.
//!
//! `TracingOptions::from_env` reads `LANGFUSE_TRACING_ENVIRONMENT`,
//! `LANGFUSE_RELEASE`, `LANGFUSE_SAMPLE_RATE` and `LANGFUSE_TRACING_ENABLED`,
//! among others; all are optional.
//!
//! **Docs:** <https://langfuse.com/docs/observability/sdk/instrumentation>,
//! <https://langfuse.com/docs/observability/features/observation-types>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example otel_tracing --features otel
//! ```

use std::time::Duration;

use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
use langfuse_rust_sdk::otel::{
    Level, ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    // The tracer exports spans to the client's project in the background.
    let tracer = client.tracer(TracingOptions::from_env()?)?;

    // The root observation: its input and output are what the trace shows
    // (Langfuse v4 has no separate trace input and output).
    let question = "Where is my order #1234?";
    let agent = tracer.start_observation("support-agent", ObservationType::Agent);
    agent.set_input(&json!({"question": question}));

    // Trace-wide attributes. Langfuse queries observations directly, so they
    // must be on every observation that should be found by them:
    // `propagate_attributes` copies them to all observations started inside
    // its future.
    let user = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_session_id("support-chat-7")
        .with_tags(["rust-sdk", "example"])
        .with_trace_name("support-request");

    // `in_scope` makes `agent` the current observation while the future runs:
    // observations started inside become its children.
    let answer = agent
        .in_scope(propagate_attributes(user, async {
            let retriever = tracer.start_observation("order-lookup", ObservationType::Retriever);
            retriever.set_input(&json!({"order_id": 1234}));
            tokio::time::sleep(Duration::from_millis(40)).await;
            retriever.set_output(&json!({"status": "shipped", "eta": "tomorrow"}));
            // `end` sets the end time; an observation that is dropped without
            // it ends when its last handle goes away.
            retriever.end();

            // A generation is a model call: model, parameters and usage let
            // Langfuse compute its cost.
            let generation = tracer.start_observation("answer", ObservationType::Generation);
            generation
                .set_model("gpt-4o-mini")
                .set_model_parameters(&json!({"temperature": 0.2}))
                .set_input(&json!([
                    {"role": "system", "content": "You are a support agent."},
                    {"role": "user", "content": question}
                ]));
            tokio::time::sleep(Duration::from_millis(120)).await;
            let answer = "Your order shipped and arrives tomorrow.";
            generation
                .set_output(answer)
                .set_usage_details(&json!({"input": 32, "output": 9}));
            // Attached to this generation. Scores are queued and sent in the
            // background; `tracer.shutdown()` below flushes them.
            generation.score(CreateScoreRequest {
                comment: Some("Mentions the delivery date".into()),
                ..CreateScoreRequest::new("helpfulness", CreateScoreValue::Numeric(0.9))
            });
            generation.end();

            // An event is a point in time. Levels are DEBUG, DEFAULT, WARNING
            // and ERROR; the status message explains the level.
            let event = tracer.start_observation("cache-miss", ObservationType::Event);
            event
                .set_level(Level::Warning)
                .set_status_message("order cache was cold");
            event.end();
            answer
        }))
        .await;

    agent.set_output(answer);
    agent.end();

    // Short-lived program: export the remaining spans and send the queued
    // scores before exiting. Long-running services call this on shutdown.
    tracer.shutdown().await;
    println!("Traced '{}' (trace {})", agent.id(), agent.trace_id());
    // Ingestion is asynchronous: the trace shows up in the UI after a few seconds.
    println!("URL: {}", client.trace_url(agent.trace_id()).await?);
    Ok(())
}
