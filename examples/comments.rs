//! Attach a comment to a trace, fetch it by ID, then list the comments of
//! that trace. Comments are the notes and discussions of the Langfuse UI; they
//! can be attached to traces, observations, sessions and prompts.
//!
//! **Writes data:** creates one comment, which is kept. The public API has no
//! delete endpoint for comments; remove it in the Langfuse UI if needed.
//!
//! The trace comes from `LANGFUSE_TRACE_ID`, or else from the most recent
//! observation of the last 24 hours. Without either the example explains what
//! is missing and exits.
//!
//! **Docs:** <https://langfuse.com/docs/observability/features/comments>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example comments
//! ```

use chrono::{Duration, SecondsFormat, Utc};
use langfuse_rust_sdk::api::comments::ListCommentsParams;
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::models::comments::CreateCommentRequest;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// `LANGFUSE_TRACE_ID`, or the trace of the most recent observation of the
/// last 24 hours.
async fn find_trace_id(
    client: &LangfuseClient,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Ok(id) = std::env::var("LANGFUSE_TRACE_ID") {
        return Ok(Some(id));
    }
    // Without `fromStartTime` the v2 API searches from 2020-01-01.
    let since = (Utc::now() - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let page = client
        .observations
        .list(&ListObservationsParams {
            limit: Some(20),
            from_start_time: Some(since),
            ..Default::default()
        })
        .await?;
    Ok(page.data.into_iter().find_map(|obs| obs.trace_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // The comment request needs the project ID. A project-scoped key returns
    // exactly its own project.
    let projects = client.projects.list().await?;
    let project = projects.data.first().ok_or("no project for this API key")?;

    let Some(trace_id) = find_trace_id(&client).await? else {
        println!(
            "No traces in the last 24 hours. Set LANGFUSE_TRACE_ID or ingest one first, e.g. `cargo run --example otel_ingestion`."
        );
        return Ok(());
    };
    println!("Project {} / trace {trace_id}", project.name);

    // Object types are TRACE, OBSERVATION, SESSION or PROMPT, and the object
    // ID must belong to an existing object of that type. The content may use
    // markdown and is limited to 5,000 characters.
    let created = client
        .comments
        .create(&CreateCommentRequest::new(
            project.id.clone(),
            "TRACE",
            trace_id.clone(),
            "Reviewed from the Rust SDK: **output looks correct**.",
        ))
        .await?;
    println!("Created comment {}", created.id);

    let comment = client.comments.get(&created.id).await?;
    println!(
        "Fetched: [{:?} {}] {}",
        comment.object_type, comment.object_id, comment.content
    );

    // All comments of one object: filter by type and ID together. A comment
    // has an author only when the request names one (`author_user_id`, a
    // member of the organization); this one has none.
    let comments = client
        .comments
        .list(&ListCommentsParams {
            object_type: Some("TRACE".into()),
            object_id: Some(trace_id),
            limit: Some(20),
            ..Default::default()
        })
        .await?;
    println!("\nComments on this trace ({}):", comments.meta.total_items);
    for c in &comments.data {
        println!(
            "  {} by {}: {}",
            c.created_at.format("%Y-%m-%d %H:%M"),
            c.author_user_id.as_deref().unwrap_or("API"),
            c.content
        );
    }

    Ok(())
}
