//! Trace a chat request with nested observations (agent, retriever, generation,
//! event), user and session attributes, and print the trace URL.
//!
//! Writes data: one trace, kept so you can open it. Delete it in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example otel_tracing --features otel
//! ```

use std::time::Duration;

use langfuse_rust_sdk::otel::{
    Level, ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    // LANGFUSE_TRACING_ENVIRONMENT, LANGFUSE_RELEASE, LANGFUSE_SAMPLE_RATE, ...
    let tracer = client.tracer(TracingOptions::from_env()?)?;

    let question = "Where is my order #1234?";
    let agent = tracer.start_observation("support-agent", ObservationType::Agent);
    agent.set_input(&json!({"question": question}));

    let user = PropagatedAttributes {
        user_id: Some("user-42".into()),
        session_id: Some("support-chat-7".into()),
        tags: vec!["rust-sdk".into(), "example".into()],
        trace_name: Some("support-request".into()),
        ..Default::default()
    };

    let answer = agent
        .in_scope(propagate_attributes(user, async {
            let retriever = tracer.start_observation("order-lookup", ObservationType::Retriever);
            retriever.set_input(&json!({"order_id": 1234}));
            tokio::time::sleep(Duration::from_millis(40)).await;
            retriever.set_output(&json!({"status": "shipped", "eta": "tomorrow"}));
            retriever.end();

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
            generation.end();

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

    // Short-lived program: send everything before exiting.
    tracer.shutdown().await;
    println!("Traced '{}' (trace {})", agent.id(), agent.trace_id());
    // Ingestion is asynchronous: the trace shows up in the UI after a few seconds.
    println!("URL: {}", client.trace_url(agent.trace_id()).await?);
    Ok(())
}
