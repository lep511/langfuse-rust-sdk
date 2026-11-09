//! Attach a comment to a trace, then list and fetch the comments of that trace.
//!
//! **Writes data:** creates one comment. The API has no delete endpoint for
//! comments, so remove it in the Langfuse UI if needed.
//!
//! The trace comes from `LANGFUSE_TRACE_ID`, or else from the most recent
//! observation in the project.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example comments
//! ```

use langfuse_sdk::api::comments::ListCommentsParams;
use langfuse_sdk::api::observations::ListObservationsParams;
use langfuse_sdk::models::comments::CreateCommentRequest;
use langfuse_sdk::{ClientConfig, LangfuseClient};

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

/// `LANGFUSE_TRACE_ID`, or the trace of the most recent observation.
async fn find_trace_id(
    client: &LangfuseClient,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Ok(id) = std::env::var("LANGFUSE_TRACE_ID") {
        return Ok(Some(id));
    }
    let page = client
        .observations
        .list(&ListObservationsParams {
            limit: Some(20),
            ..Default::default()
        })
        .await?;
    Ok(page.data.into_iter().find_map(|obs| obs.trace_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // A project-scoped key returns exactly its own project.
    let projects = client.projects.list().await?;
    let project = projects.data.first().ok_or("no project for this API key")?;

    let Some(trace_id) = find_trace_id(&client).await? else {
        println!("No traces yet. Ingest one first, e.g. `cargo run --example otel_ingestion`.");
        return Ok(());
    };
    println!("Project {} / trace {trace_id}", project.name);

    // Object types are TRACE, OBSERVATION, SESSION or PROMPT.
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
