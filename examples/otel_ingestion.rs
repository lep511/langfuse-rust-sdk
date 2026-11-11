//! Build one OpenTelemetry span by hand, send it to Langfuse's OTLP endpoint
//! (`POST /api/public/otel/v1/traces`, OTLP/HTTP JSON) and print its URL in
//! the UI. No `otel` feature and no OpenTelemetry SDK involved: this is the
//! low-level path, useful to forward spans that some other system produced.
//! To instrument your own code, use the tracer instead (see `otel_tracing`).
//!
//! **Writes data:** one trace with a single generation, kept so you can open
//! it. Delete it in the UI or with `client.traces.delete(trace_id)`.
//!
//! How Langfuse reads the span:
//!
//! - `langfuse.observation.type` makes it a generation (the other accepted
//!   values are `span`, `event`, `embedding`, `agent`, `tool`, `chain`,
//!   `retriever`, `guardrail` and `evaluator`).
//! - `langfuse.observation.input` / `.output` hold the payload, as a string or
//!   a JSON string.
//! - `gen_ai.request.model` becomes the model, other `gen_ai.request.*`
//!   attributes the model parameters and `gen_ai.usage.*` the usage, from
//!   which Langfuse computes the cost when it has a price for the model.
//! - `langfuse.trace.name` and `langfuse.trace.tags` are trace-level
//!   attributes. Langfuse v4 has no separate trace record, so set them on
//!   every span that must be found by them.
//! - Attributes Langfuse does not map (`gen_ai.system` here) are kept under
//!   `metadata.attributes`, resource attributes under
//!   `metadata.resourceAttributes`.
//!
//! Export a span once, when it is complete: Langfuse v4 does not deduplicate
//! span IDs, so sending the same span again creates a second observation.
//!
//! **Docs:** <https://langfuse.com/integrations/native/opentelemetry>
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

    // W3C trace context IDs: 32 lowercase hex characters for the trace, 16
    // for the span. `None` means random; a seed gives a deterministic ID.
    let trace_id = create_trace_id(None);
    let span_id = create_observation_id(None);

    // OTLP timestamps are nanoseconds since the Unix epoch, as strings.
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let start_ns = now_ns.to_string();
    let end_ns = (now_ns + 2_500_000_000u128).to_string(); // +2.5s

    let span = OtelSpan {
        kind: Some(3), // SPAN_KIND_CLIENT
        attributes: Some(vec![
            OtelKeyValue::string("langfuse.observation.type", "generation"),
            OtelKeyValue::string("langfuse.trace.name", "rust-sdk-otel-example"),
            OtelKeyValue::strings("langfuse.trace.tags", ["rust-sdk", "example"]),
            // Structured values travel as JSON strings.
            OtelKeyValue::json(
                "langfuse.observation.input",
                &serde_json::json!([{"role": "user", "content": "Hello!"}]),
            )?,
            OtelKeyValue::string("gen_ai.system", "anthropic"),
            OtelKeyValue::string("gen_ai.request.model", "claude-sonnet-5-5"),
            OtelKeyValue::double("gen_ai.request.temperature", 0.7),
            OtelKeyValue::int("gen_ai.usage.input_tokens", 150),
            OtelKeyValue::int("gen_ai.usage.output_tokens", 320),
        ]),
        status: Some(OtelStatus {
            code: Some(1), // STATUS_CODE_OK; 2 (error) gives the observation level ERROR
            message: None,
        }),
        // No parent span ID: this span is the root observation of its trace.
        ..OtelSpan::new(&trace_id, span_id, "llm-generation", start_ns, end_ns)
    };

    // The OTLP envelope: resource -> instrumentation scope -> spans.
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

    // Sent with Basic auth and `x-langfuse-ingestion-version: 4`, the header
    // that selects the real-time v4 ingestion path.
    client.ingestion.ingest_traces(&request).await?;
    println!("Ingested OTel trace: {trace_id}");
    println!("  Span: llm-generation (2.5s, 150 in / 320 out tokens, tags rust-sdk + example)");
    // Ingestion is asynchronous: the trace shows up in the UI after a few seconds.
    println!("  URL:  {}", client.trace_url(&trace_id).await?);

    Ok(())
}
