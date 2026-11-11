//! Multi-modal tracing: attach an image and a text file to a traced
//! generation, then read the stored observation back and resolve the image
//! reference into a data URI again.
//!
//! **Writes data:** one trace, kept so you can see the image in the UI, and
//! two media files. Delete the trace in the UI or with
//! `client.traces.delete(trace_id)`. Media files are identified by project,
//! content type and SHA-256 of their content, so running the example again
//! uploads nothing new.
//!
//! **Needs:** the `otel` feature.
//!
//! How it works: the exporter uploads every media object found in the input,
//! output or metadata of a span (and every base64 data URI) to Langfuse's
//! object storage, and stores a reference string in its place:
//! `@@@langfuseMedia:type=image/png|id=<media id>|source=base64_data_uri@@@`.
//! Set `LANGFUSE_MEDIA_UPLOAD_ENABLED=false` to turn the uploads off.
//!
//! **Docs:** <https://langfuse.com/docs/observability/features/multi-modality>
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

    // Media can come from a data URI, from bytes or from a file
    // (`LangfuseMedia::from_file`). Plain base64 data URIs inside traced data
    // are detected and uploaded too, without wrapping them.
    let image = LangfuseMedia::from_data_uri(&format!("data:image/png;base64,{RED_PIXEL_PNG}"))?;
    let report =
        LangfuseMedia::from_bytes(b"Pixel color: red".to_vec(), MediaContentType::TextPlain);

    // The observations of this run start after `since`; it bounds the query
    // below (without `fromStartTime` the v2 API searches from 2020-01-01).
    let since =
        (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Secs, true);
    // Put the media where it belongs in the payload: the image inside an
    // OpenAI-style message (the UI renders it inline), the file in metadata.
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

    // `flush` exports the span and then waits for the uploads the export
    // queued, so the references below point at files that exist.
    tracer.flush().await;
    println!(
        "Traced generation {} with an image and a text file",
        generation.id()
    );
    println!("URL: {}", client.trace_url(&trace_id).await?);

    // Read the input back: the image is now a reference string. The v2 API
    // returns input and output as raw strings (`parseIoAsJson=true` is
    // rejected with 400), so the JSON input is parsed here. Ingestion is
    // asynchronous, hence the polling.
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
            // Downloads every referenced file and puts it back as a base64
            // data URI, e.g. to replay the request against a model.
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
