//! Create numeric and categorical scores for a trace in one batch.
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

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    let numeric = CreateScoreRequest {
        trace_id: Some("example-trace-id".into()),
        data_type: Some(ScoreDataType::Numeric),
        comment: Some("High accuracy on test set".into()),
        environment: Some("production".into()),
        ..CreateScoreRequest::new("accuracy", CreateScoreValue::Numeric(0.95))
    };
    let categorical = CreateScoreRequest {
        trace_id: Some("example-trace-id".into()),
        data_type: Some(ScoreDataType::Categorical),
        comment: Some("User feedback was positive".into()),
        ..CreateScoreRequest::new("sentiment", CreateScoreValue::String("positive".into()))
    };

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
