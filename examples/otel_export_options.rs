//! Export options of the tracer, end to end:
//!
//! - gzip-compressed exports (the default; `LANGFUSE_OTEL_COMPRESSION=none`
//!   turns it off for proxies that reject compressed requests);
//! - a tracer provider of your own that also feeds another exporter, through
//!   `LangfuseSpanProcessor` and `LangfuseTracer::with_provider`;
//! - `mask_otel_spans`, which redacts span attributes before they leave the
//!   process, including those of spans created by other instrumentation.
//!
//! The example then waits until Langfuse has stored the trace, checks through
//! the API that no secret arrived, and deletes the trace. It exits with an
//! error if the check fails.
//!
//! **Writes data:** one trace with three observations, deleted at the end
//! (one request of the trace-data-deletion rate limit; Langfuse removes the
//! data asynchronously, usually within 15 minutes). Set `KEEP_TRACE=1` to keep
//! it and open the printed URL instead.
//!
//! **Needs:** the `otel` feature.
//!
//! **Docs:** <https://langfuse.com/docs/observability/features/masking>,
//! <https://langfuse.com/docs/observability/sdk/advanced-features>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example otel_export_options --features otel
//! ```

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::models::observations::ObservationV2;
use langfuse_rust_sdk::otel::opentelemetry::KeyValue;
use langfuse_rust_sdk::otel::opentelemetry::trace::{Span as _, Tracer as _, TracerProvider as _};
use langfuse_rust_sdk::otel::opentelemetry_sdk::error::OTelSdkResult;
use langfuse_rust_sdk::otel::opentelemetry_sdk::trace::{
    SdkTracerProvider, SpanData, SpanExporter,
};
use langfuse_rust_sdk::otel::{
    Compression, LangfuseSpanProcessor, LangfuseTracer, ObservationType, TracingOptions,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};
use serde_json::json;

/// Stands in for another backend sharing the provider: prints each span.
#[derive(Debug)]
struct PrintExporter;

impl SpanExporter for PrintExporter {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        for span in batch {
            println!(
                "  [other backend] {} ({})",
                span.name,
                span.span_context.span_id()
            );
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // Gzip is the default (and `from_env` reads LANGFUSE_OTEL_COMPRESSION);
    // it is set here only to make the example explicit.
    //
    // The mask runs in the export task on each batch, after media upload and
    // right before the request is built, so it sees the final attributes of
    // every span. If it panics the whole batch is dropped: nothing unmasked
    // is ever sent.
    let options = TracingOptions::from_env()?
        .with_compression(Compression::Gzip)
        .with_mask_otel_spans(|spans| {
            for span in spans {
                // Observation metadata is flattened to one attribute per key,
                // `langfuse.observation.metadata.<key>`, with a JSON-encoded
                // value: the replacement is a JSON string too.
                if span
                    .remove_attribute("langfuse.observation.metadata.api_key")
                    .is_some()
                {
                    span.set_attribute(KeyValue::new(
                        "langfuse.observation.metadata.api_key",
                        "\"[redacted]\"",
                    ));
                }
                // An attribute of a span this SDK did not create: drop it and
                // leave a marker instead.
                if span
                    .remove_attribute("http.request.header.authorization")
                    .is_some()
                {
                    span.set_attribute(KeyValue::new("security.redacted", true));
                }
            }
        });
    // Your own provider: add the Langfuse processor next to whatever else
    // you export to. `client.tracer(options)` is the shortcut that builds a
    // provider with only the Langfuse processor.
    let processor = LangfuseSpanProcessor::new(&client, options)?;
    let provider = SdkTracerProvider::builder()
        .with_span_processor(processor.clone())
        .with_simple_exporter(PrintExporter)
        .build();
    let tracer = LangfuseTracer::with_provider(&processor, provider.clone())?;

    println!("Tracing (spans go to Langfuse and to the other backend):");
    // The observations of this run start after `since`; it bounds the query
    // that waits for them (without `fromStartTime` the v2 API searches from
    // 2020-01-01).
    let since =
        (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let request = tracer.start_observation("export-options-demo", ObservationType::Agent);
    request.set_input(&json!({"question": "Which plan am I on?"}));
    let trace_id = request.trace_id().to_owned();
    request
        .in_scope(async {
            let lookup = tracer.start_observation("account-lookup", ObservationType::Tool);
            lookup
                .set_metadata(&json!({"api_key": "sk-very-secret", "region": "eu"}))
                .set_output(&json!({"plan": "pro"}));
            lookup.end();

            // A span of other instrumentation on the same provider. Langfuse
            // exports it because it carries a `gen_ai.*` attribute; spans of
            // unrelated libraries are filtered out by default.
            let mut http = provider.tracer("my-http-client").start("POST /v1/chat");
            http.set_attribute(KeyValue::new("gen_ai.system", "openai"));
            http.set_attribute(KeyValue::new(
                "http.request.header.authorization",
                "Bearer sk-very-secret",
            ));
            http.end();
        })
        .await;
    request.set_output("You are on the pro plan.");
    request.end();

    // `with_provider` leaves the provider running on `tracer.shutdown()`,
    // because it is yours: shut it down yourself.
    tracer.shutdown().await;
    let _ = provider.shutdown();

    println!("Waiting for Langfuse to store trace {trace_id}...");
    let observations = wait_for_observations(&client, &trace_id, &since, 3).await?;
    let mut ok = observations.len() == 3;
    for observation in &observations {
        let name = observation.name.as_deref().unwrap_or("?");
        let metadata = observation.metadata.clone().unwrap_or_default();
        println!("  {name}: metadata {metadata}");
        let text = metadata.to_string();
        if text.contains("sk-very-secret") {
            println!("    ✗ the secret reached Langfuse");
            ok = false;
        }
        if name == "account-lookup" && metadata["api_key"] != "[redacted]" {
            println!("    ✗ api_key was not redacted");
            ok = false;
        }
    }
    println!(
        "{}",
        if ok {
            "✓ gzip export stored, secrets redacted"
        } else {
            "✗ unexpected result"
        }
    );

    if std::env::var("KEEP_TRACE").is_ok_and(|v| v == "1") {
        println!("Trace kept: {}", client.trace_url(&trace_id).await?);
    } else {
        client.traces.delete(&trace_id).await?;
        println!("Trace deleted");
    }
    if !ok {
        return Err("verification failed".into());
    }
    Ok(())
}

/// Poll the observations of `trace_id` (started at or after `since`) for up to
/// a minute until `count` are stored.
async fn wait_for_observations(
    client: &LangfuseClient,
    trace_id: &str,
    since: &str,
    count: usize,
) -> Result<Vec<ObservationV2>, LangfuseError> {
    let params = ListObservationsParams {
        trace_id: Some(trace_id.to_owned()),
        from_start_time: Some(since.to_owned()),
        fields: Some("core,basic,metadata".into()),
        ..Default::default()
    };
    let mut observations = Vec::new();
    for _ in 0..20 {
        observations = client.observations.list(&params).await?.data;
        if observations.len() >= count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    Ok(observations)
}
