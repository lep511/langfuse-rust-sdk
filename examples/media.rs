//! Attach an image to a traced generation, then read it back as a data URI.
//!
//! Writes data: one trace (kept so you can see the image in the UI) and the
//! image file. Delete the trace in the UI or with `client.traces.delete`;
//! uploaded files are content-addressed and stay in the project's storage.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example media --features otel
//! ```

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::media::LangfuseMedia;
use langfuse_rust_sdk::models::media::MediaContentType;
use langfuse_rust_sdk::otel::{ObservationType, TracingOptions};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

/// A 1×1 red PNG.
const RED_PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = client.tracer(TracingOptions::from_env()?)?;

    // Media can come from bytes, a file or a data URI; base64 data URIs in
    // traced data are detected and uploaded automatically too.
    let image = LangfuseMedia::from_data_uri(&format!("data:image/png;base64,{RED_PIXEL_PNG}"))?;
    let report =
        LangfuseMedia::from_bytes(b"Pixel color: red".to_vec(), MediaContentType::TextPlain);

    // The observations of this run start after `since`; it bounds the query
    // below (without `fromStartTime` the v2 API searches from 2020-01-01).
    let since =
        (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let generation = tracer.start_observation("describe-image", ObservationType::Generation);
    generation
        .set_model("gpt-4o-mini")
        .set_input(&json!([{
            "role": "user",
            "content": [
                {"type": "text", "text": "What color is this pixel?"},
                {"type": "image_url", "image_url": {"url": image}}
            ]
        }]))
        .set_output("It is red.")
        .set_metadata(&json!({"report": report}));
    generation.end();
    let trace_id = generation.trace_id().to_string();

    // Export the span, then finish the uploads queued during export.
    tracer.flush().await;
    println!(
        "Traced generation {} with an image and a text file",
        generation.id()
    );
    println!("URL: {}", client.trace_url(&trace_id).await?);

    // Read the input back: the image is stored as a reference string. Langfuse
    // v4 returns input and output as raw strings (`parseIoAsJson=true` is
    // rejected with 400), so the JSON input is parsed here.
    let params = ListObservationsParams {
        trace_id: Some(trace_id.clone()),
        from_start_time: Some(since),
        fields: Some("core,io".into()),
        ..Default::default()
    };
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if let Some(observation) = client
            .observations
            .list(&params)
            .await?
            .data
            .into_iter()
            .next()
        {
            let input = match observation.input {
                Some(serde_json::Value::String(raw)) => {
                    serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw))
                }
                other => other.unwrap_or_default(),
            };
            println!("Stored input: {input}");
            let resolved = client.media.resolve_references(&input).await;
            let url = &resolved[0]["content"][1]["image_url"]["url"];
            println!(
                "Resolved image: {}...",
                url.as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(40)
                    .collect::<String>()
            );
            break;
        }
    }

    tracer.shutdown().await;
    client.shutdown().await;
    Ok(())
}
