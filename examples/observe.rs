//! Trace plain functions with the `#[observe]` attribute, the counterpart of
//! the Python SDK's `@observe` decorator: an agent that calls a retriever, a
//! failing tool and a streamed generation, with the skills available to the
//! agent attached to the trace. Prints the trace URL.
//!
//! **Writes data:** one trace (four observations), kept so you can open it.
//! Delete it in the UI or with `client.traces.delete(trace_id)`.
//!
//! **Needs:** the `macros` feature (it includes `otel`).
//!
//! Each call of an observed function becomes an observation named after the
//! function, with its arguments as input and its return value as output; the
//! observations nest the way the calls do. Set
//! `LANGFUSE_OBSERVE_DECORATOR_IO_CAPTURE_ENABLED=false` to stop recording
//! arguments and results.
//!
//! **Docs:** <https://langfuse.com/docs/observability/sdk/instrumentation>,
//! <https://langfuse.com/docs/skill-management/tracing>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example observe --features macros
//! ```

use futures::{Stream, StreamExt};
use langfuse_rust_sdk::ids::create_trace_id;
use langfuse_rust_sdk::observe;
use langfuse_rust_sdk::otel::{
    Observation, PropagatedAttributes, TraceContext, TracingOptions, in_trace, propagate_attributes,
};
use langfuse_rust_sdk::skills::{SkillReference, discover_skills};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
#[error("carrier API timed out for order {0}")]
struct CarrierTimeout(u32);

/// Input `{"order_id": ..}`, output the returned JSON. `as_type` picks the
/// observation type; without it the function is a plain span.
#[observe(as_type = "retriever")]
async fn find_order(order_id: u32) -> serde_json::Value {
    json!({"order_id": order_id, "status": "shipped", "carrier": "ACME"})
}

/// Returning `Err` gives the observation the level ERROR and the error's
/// message as status message; `Ok(value)` would record `value` as output.
#[observe(as_type = "tool")]
async fn track_package(order_id: u32) -> Result<String, CarrierTimeout> {
    Err(CarrierTimeout(order_id))
}

/// A streamed answer: the observation stays open until the stream ends (or
/// is dropped), and its output is the concatenated text. `name` overrides the
/// function name.
#[observe(name = "compose-answer", as_type = "generation")]
fn compose_answer(order: &serde_json::Value) -> impl Stream<Item = String> {
    // The observation this function runs in: set what the arguments and the
    // return value cannot tell, here the model and the token usage.
    if let Some(generation) = Observation::current() {
        generation
            .set_model("gpt-4o-mini")
            .set_usage_details(&json!({"input": 42, "output": 9}));
    }
    let status = order["status"].as_str().unwrap_or("unknown").to_owned();
    futures::stream::iter(["Your order ", "is ", "on its way: "].map(str::to_owned))
        .chain(futures::stream::once(async move { status }))
}

/// The root observation; the three calls inside become its children.
#[observe(name = "support-agent", as_type = "agent")]
async fn support_agent(question: &str, order_id: u32) -> String {
    let order = find_order(order_id).await;
    // A failed tool call is recorded as an error; the agent carries on.
    let tracking = track_package(order_id).await.unwrap_or_default();
    let answer: String = compose_answer(&order).collect().await;
    format!("{answer} {tracking}").trim().to_owned()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    // The skills the agent can load: every subdirectory of ./skills with a
    // SKILL.md (versions come from `langfuse-skills.lock`, which the Langfuse
    // CLI writes). The directory is optional, so fall back to a made-up skill.
    let mut skills = discover_skills("skills");
    if skills.is_empty() {
        skills.push(SkillReference::new("order-lookup").with_version(3));
    }
    let tracer = client.tracer(TracingOptions::from_env()?.with_skills(skills))?;
    // Observed functions called outside any observation use the global
    // tracer; without one they run untraced.
    tracer.install_global();

    // Copied to every observation of the call below. `attach_skills` adds the
    // tracer's skill list, so Langfuse can link the trace to those skills.
    let attributes = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_session_id("support-chat-8")
        .with_tags(["rust-sdk", "example", "observe"])
        .with_trace_name("observe-example")
        .with_attach_skills(true);
    // A trace ID chosen up front, so the URL is known before anything is
    // sent. `create_trace_id(Some("request-123"))` derives it from a seed,
    // e.g. a request ID, and always gives the same ID for the same seed.
    let trace_id = create_trace_id(None);
    let answer = in_trace(
        TraceContext::new(trace_id.clone()),
        propagate_attributes(attributes, support_agent("Where is my order?", 1234)),
    )
    .await;
    println!("Answer: {answer}");

    // Export the spans before the program exits.
    tracer.shutdown().await;
    println!("URL: {}", client.trace_url(&trace_id).await?);
    Ok(())
}
