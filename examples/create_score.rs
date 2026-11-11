//! Create a numeric and a categorical score for a trace in one request:
//! `POST /api/public/scores` takes a batch and ingests it asynchronously.
//!
//! **Writes data:** two scores, `accuracy` and `sentiment`, on the placeholder
//! trace ID `example-trace-id`. They are kept. Put a real trace ID (32 hex
//! characters) in `TRACE_ID` to see them on a trace in the UI; find their IDs
//! with the `list_scores` example and remove them with
//! `client.scores.delete(score_id)`.
//!
//! For scores sent from application code prefer the queue behind
//! `client.score(..)` / `observation.score(..)` (see the `otel_tracing`
//! example): it batches in the background and gives every score an ID, so a
//! retried request cannot create duplicates.
//!
//! **Docs:** <https://langfuse.com/docs/evaluation/evaluation-methods/scores-via-sdk>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example create_score
//! ```

use langfuse_rust_sdk::models::enums::ScoreDataType;
use langfuse_rust_sdk::models::scores::{
    CreateScoreRequest, CreateScoreValue, CreateScoresResponse,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

/// The trace the scores are attached to.
const TRACE_ID: &str = "example-trace-id";

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // `new(name, value)` fills the required fields; the struct update syntax
    // adds the optional ones. A score needs a target: here the trace
    // (`observation_id`, `session_id` and `dataset_run_id` are the others).
    let numeric = CreateScoreRequest {
        trace_id: Some(TRACE_ID.into()),
        data_type: Some(ScoreDataType::Numeric),
        comment: Some("High accuracy on test set".into()),
        environment: Some("production".into()),
        ..CreateScoreRequest::new("accuracy", CreateScoreValue::Numeric(0.95))
    };
    // Categorical scores carry a string value.
    let categorical = CreateScoreRequest {
        trace_id: Some(TRACE_ID.into()),
        data_type: Some(ScoreDataType::Categorical),
        comment: Some("User feedback was positive".into()),
        ..CreateScoreRequest::new("sentiment", CreateScoreValue::String("positive".into()))
    };

    // 202: the whole batch was accepted. 207: some scores were rejected, each
    // with its reason; do not resend the whole batch blindly, the accepted
    // ones would be created twice.
    match client.scores.create(&[numeric, categorical]).await? {
        CreateScoresResponse::Accepted(resp) => {
            println!("Batch accepted: {}", resp.message);
        }
        CreateScoresResponse::BatchResults(results) => {
            println!(
                "Accepted {}, rejected {}",
                results.accepted, results.rejected
            );
            for error in &results.errors {
                println!("  rejected: {}", error.message);
            }
        }
    }

    Ok(())
}
