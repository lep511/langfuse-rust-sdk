//! Ingest an OpenTelemetry span into Langfuse and print its URL in the UI.
//!
//! Writes data: one trace, kept so you can open it. Delete it in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example otel_ingestion
//! ```

use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
use langfuse_rust_sdk::models::ingestion::*;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    let trace_id = create_trace_id(None);
    let span_id = create_observation_id(None);

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let start_ns = now_ns.to_string();
    let end_ns = (now_ns + 2_500_000_000u128).to_string(); // +2.5s

    let span = OtelSpan {
        kind: Some(3), // CLIENT
        attributes: Some(vec![
            OtelKeyValue::string("langfuse.observation.type", "generation"),
            OtelKeyValue::string("langfuse.trace.name", "rust-sdk-otel-example"),
            OtelKeyValue::strings("langfuse.trace.tags", ["rust-sdk", "example"]),
            OtelKeyValue::json(
                "langfuse.observation.input",
                &serde_json::json!([{"role": "user", "content": "Hello!"}]),
            )?,
            OtelKeyValue::string("gen_ai.system", "anthropic"),
            OtelKeyValue::string("gen_ai.request.model", "claude-3-sonnet"),
            OtelKeyValue::double("gen_ai.request.temperature", 0.7),
            OtelKeyValue::int("gen_ai.usage.input_tokens", 150),
            OtelKeyValue::int("gen_ai.usage.output_tokens", 320),
        ]),
        status: Some(OtelStatus {
            code: Some(1), // OK
            message: None,
        }),
        ..OtelSpan::new(&trace_id, span_id, "llm-generation", start_ns, end_ns)
    };

    let request = IngestTracesRequest {
        resource_spans: vec![OtelResourceSpan {
            resource: Some(OtelResource {
                attributes: Some(vec![
                    OtelKeyValue::string("service.name", "rust-sdk-example"),
                    OtelKeyValue::string("service.version", env!("CARGO_PKG_VERSION")),
                ]),
            }),
            scope_spans: vec![OtelScopeSpan {
                scope: Some(OtelInstrumentationScope {
                    name: Some("langfuse-sdk-rust".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    attributes: None,
                }),
                spans: vec![span],
            }],
        }],
    };

    client.ingestion.ingest_traces(&request).await?;
    println!("Ingested OTel trace: {trace_id}");
    println!("  Span: llm-generation (2.5s, 150 in / 320 out tokens, tags rust-sdk + example)");
    // Ingestion is asynchronous: the trace shows up in the UI after a few seconds.
    println!("  URL:  {}", client.trace_url(&trace_id).await?);

    Ok(())
}
