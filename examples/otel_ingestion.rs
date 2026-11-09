//! Ingest OpenTelemetry trace spans into Langfuse.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example otel_ingestion
//! ```

use langfuse_rust_sdk::models::ingestion::*;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL").unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;
    Ok(LangfuseClient::new(config)?)
}

fn string_attr(key: &str, value: &str) -> OtelKeyValue {
    OtelKeyValue {
        key: key.into(),
        value: OtelAnyValue {
            string_value: Some(value.into()),
            int_value: None,
            double_value: None,
            bool_value: None,
        },
    }
}

fn int_attr(key: &str, value: i64) -> OtelKeyValue {
    OtelKeyValue {
        key: key.into(),
        value: OtelAnyValue {
            string_value: None,
            int_value: Some(value),
            double_value: None,
            bool_value: None,
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    let trace_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let span_id = uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string();

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let start_ns = now_ns.to_string();
    let end_ns = (now_ns + 2_500_000_000u128).to_string(); // +2.5s

    let request = IngestTracesRequest {
        resource_spans: vec![OtelResourceSpan {
            resource: Some(OtelResource {
                attributes: Some(vec![
                    string_attr("service.name", "rust-sdk-example"),
                    string_attr("service.version", "0.1.0"),
                ]),
            }),
            scope_spans: vec![OtelScopeSpan {
                scope: Some(OtelInstrumentationScope {
                    name: Some("langfuse-sdk-rust".into()),
                    version: Some("0.1.0".into()),
                    attributes: None,
                }),
                spans: vec![OtelSpan {
                    trace_id: trace_id.clone(),
                    span_id,
                    parent_span_id: None,
                    name: "llm-generation".into(),
                    kind: Some(3), // CLIENT
                    start_time_unix_nano: start_ns,
                    end_time_unix_nano: end_ns,
                    attributes: Some(vec![
                        string_attr("gen_ai.system", "anthropic"),
                        string_attr("gen_ai.request.model", "claude-3-sonnet"),
                        int_attr("gen_ai.usage.input_tokens", 150),
                        int_attr("gen_ai.usage.output_tokens", 320),
                        string_attr("gen_ai.request.temperature", "0.7"),
                    ]),
                    status: Some(OtelStatus {
                        code: Some(1), // OK
                        message: None,
                    }),
                }],
            }],
        }],
    };

    client.ingestion.ingest_traces(&request).await?;
    println!("Ingested OTel trace: {trace_id}");
    println!("  Span: llm-generation (2.5s, 150 in / 320 out tokens)");

    Ok(())
}
