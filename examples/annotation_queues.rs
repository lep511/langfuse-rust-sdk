//! Human-review workflow with annotation queues: reuse (or create) a queue,
//! add a trace to it, mark the item as completed, and remove it again.
//!
//! **Writes data:** the queue `rust-sdk-example-queue` is created on the first
//! run and reused afterwards (the API cannot delete queues). If the project has
//! no score config, a numeric one named `rust-sdk-review-quality` is created.
//! The queue item is deleted at the end.
//!
//! The trace comes from `LANGFUSE_TRACE_ID`, or else from the most recent
//! observation of the last 24 hours. Set `LANGFUSE_EXAMPLE_USER_ID` to also
//! assign and unassign a reviewer.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example annotation_queues
//! ```

use chrono::{Duration, SecondsFormat, Utc};
use langfuse_rust_sdk::api::annotation_queues::ListQueueItemsParams;
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::models::annotation_queues::{
    AnnotationQueue, AnnotationQueueObjectType, AnnotationQueueStatus,
    CreateAnnotationQueueItemRequest, CreateAnnotationQueueRequest,
    UpdateAnnotationQueueItemRequest,
};
use langfuse_rust_sdk::models::enums::ScoreConfigDataType;
use langfuse_rust_sdk::models::score_configs::CreateScoreConfigRequest;
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

const QUEUE_NAME: &str = "rust-sdk-example-queue";

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// An existing active score config, or a new numeric one.
async fn score_config_id(client: &LangfuseClient) -> Result<String, Box<dyn std::error::Error>> {
    let configs = client
        .score_configs
        .list(&PageParams {
            page: Some(1),
            limit: Some(50),
        })
        .await?;
    if let Some(config) = configs.data.iter().find(|c| !c.is_archived) {
        println!("Using score config '{}'", config.name);
        return Ok(config.id.clone());
    }
    let created = client
        .score_configs
        .create(&CreateScoreConfigRequest {
            name: "rust-sdk-review-quality".into(),
            data_type: ScoreConfigDataType::Numeric,
            categories: None,
            min_value: Some(0.0),
            max_value: Some(1.0),
            description: Some("Created by the Rust SDK annotation_queues example".into()),
        })
        .await?;
    println!("Created score config '{}'", created.name);
    Ok(created.id)
}

/// The example queue, created on first use.
async fn example_queue(
    client: &LangfuseClient,
) -> Result<AnnotationQueue, Box<dyn std::error::Error>> {
    let mut page = 1;
    loop {
        let queues = client
            .annotation_queues
            .list_queues(&PageParams {
                page: Some(page),
                limit: Some(50),
            })
            .await?;
        if let Some(queue) = queues.data.into_iter().find(|q| q.name == QUEUE_NAME) {
            println!("Reusing queue '{}'", queue.name);
            return Ok(queue);
        }
        if page >= queues.meta.total_pages {
            break;
        }
        page += 1;
    }
    let config_id = score_config_id(client).await?;
    let queue = client
        .annotation_queues
        .create_queue(&CreateAnnotationQueueRequest {
            name: QUEUE_NAME.into(),
            description: Some("Traces to review, filled by the Rust SDK example".into()),
            score_config_ids: vec![config_id],
        })
        .await?;
    println!("Created queue '{}'", queue.name);
    Ok(queue)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // The most recent observation of the last 24 hours: without
    // `fromStartTime` the v2 API searches from 2020-01-01.
    let since = (Utc::now() - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let trace_id = match std::env::var("LANGFUSE_TRACE_ID") {
        Ok(id) => Some(id),
        Err(_) => client
            .observations
            .list(&ListObservationsParams {
                limit: Some(20),
                from_start_time: Some(since),
                ..Default::default()
            })
            .await?
            .data
            .into_iter()
            .find_map(|obs| obs.trace_id),
    };
    let Some(trace_id) = trace_id else {
        println!(
            "No traces in the last 24 hours. Set LANGFUSE_TRACE_ID or ingest one first, e.g. `cargo run --example otel_ingestion`."
        );
        return Ok(());
    };

    let queue = example_queue(&client).await?;

    // Add the trace for review.
    let item = client
        .annotation_queues
        .create_item(
            &queue.id,
            &CreateAnnotationQueueItemRequest {
                object_id: trace_id.clone(),
                object_type: AnnotationQueueObjectType::Trace,
                status: None,
            },
        )
        .await?;
    println!(
        "Queued trace {trace_id} as item {} ({:?})",
        item.id, item.status
    );

    let pending = client
        .annotation_queues
        .list_items(
            &queue.id,
            &ListQueueItemsParams {
                status: Some(AnnotationQueueStatus::Pending),
                ..Default::default()
            },
        )
        .await?;
    println!("Pending items in the queue: {}", pending.meta.total_items);

    // Optionally assign a reviewer.
    if let Ok(user_id) = std::env::var("LANGFUSE_EXAMPLE_USER_ID") {
        let assignment = client
            .annotation_queues
            .create_assignment(&queue.id, &user_id)
            .await?;
        println!("Assigned user {} to the queue", assignment.user_id);
        client
            .annotation_queues
            .delete_assignment(&queue.id, &user_id)
            .await?;
        println!("Unassigned user {user_id}");
    }

    // A reviewer finished: mark the item as completed.
    let done = client
        .annotation_queues
        .update_item(
            &queue.id,
            &item.id,
            &UpdateAnnotationQueueItemRequest {
                status: Some(AnnotationQueueStatus::Completed),
            },
        )
        .await?;
    println!(
        "Item {} is now {:?} (completed at {})",
        done.id,
        done.status,
        done.completed_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "-".into())
    );

    // Clean up the item; the queue stays for the next run.
    let removed = client
        .annotation_queues
        .delete_item(&queue.id, &item.id)
        .await?;
    println!("Removed item: {}", removed.message);

    Ok(())
}
