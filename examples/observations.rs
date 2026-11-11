//! List the most recent observations of the last 24 hours as a table (type,
//! level, name, model, latency) with the v2 Observations API, the real-time
//! read path that replaces the legacy trace, session and observation
//! endpoints.
//!
//! **Read-only:** writes no data.
//!
//! Two things to know about `GET /api/public/v2/observations`:
//!
//! - **Field groups.** Only `core` and `basic` are returned by default; ask
//!   for more with `fields` (`time`, `io`, `metadata`, `model`, `usage`,
//!   `prompt`, `metrics`, `trace_context`). Fields of groups that were not
//!   requested come back as `None`.
//! - **Time window.** Always send `from_start_time`: without a lower bound
//!   the API searches from 2020-01-01.
//!
//! **Docs:** <https://langfuse.com/docs/api-and-data-platform/features/public-api>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example observations
//! ```

use chrono::{Duration, SecondsFormat, Utc};
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// At most `max` characters of `text` (never splits a UTF-8 character).
fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Newest first, bounded to the last 24 hours. `model` adds the model name
    // and `metrics` the latency to the default `core` and `basic` groups.
    let since = (Utc::now() - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let page = client
        .observations
        .list(&ListObservationsParams {
            limit: Some(10),
            from_start_time: Some(since),
            fields: Some("core,basic,model,metrics".into()),
            ..Default::default()
        })
        .await?;

    println!(
        "Recent observations of the last 24 hours ({} returned):",
        page.data.len()
    );
    println!(
        "{:<12} {:<14} {:<25} {:<20} {:>8}",
        "TYPE", "LEVEL", "NAME", "MODEL", "LATENCY"
    );
    println!("{}", "-".repeat(85));

    for obs in &page.data {
        let name = obs.name.as_deref().unwrap_or("-");
        // Observations that are not model calls come back with an empty model.
        let model = obs
            .model
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or("-");
        let level = obs
            .level
            .as_ref()
            .map(|l| format!("{:?}", l))
            .unwrap_or_else(|| "-".into());
        // Latency is in seconds.
        let latency = obs
            .latency
            .map(|l| format!("{:.1}s", l))
            .unwrap_or_else(|| "-".into());

        println!(
            "{:<12} {:<14} {:<25} {:<20} {:>8}",
            obs.r#type,
            level,
            truncate(name, 24),
            truncate(model, 19),
            latency,
        );
    }

    // Cursor pagination: pass `meta.cursor` as `cursor` to get the next page
    // (or use `observations.list_pages`, which follows it for you). A page can
    // hold fewer items than `limit`; the last page has no cursor.
    if let Some(cursor) = &page.meta.cursor {
        println!("\nNext cursor: {}", truncate(cursor, 40));
    } else {
        println!("\nNo more pages.");
    }

    Ok(())
}
