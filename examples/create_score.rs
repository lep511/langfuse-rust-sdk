//! Create numeric and categorical scores for a trace.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example create_score
//! ```

use langfuse_sdk::models::enums::ScoreDataType;
use langfuse_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
use langfuse_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;
    Ok(LangfuseClient::new(config)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Create a numeric score
    let numeric = client
        .scores
        .create(&CreateScoreRequest {
            name: "accuracy".into(),
            value: CreateScoreValue::Numeric(0.95),
            trace_id: Some("example-trace-id".into()),
            data_type: Some(ScoreDataType::Numeric),
            comment: Some("High accuracy on test set".into()),
            environment: Some("production".into()),
            id: None,
            session_id: None,
            observation_id: None,
            dataset_run_id: None,
            metadata: None,
            queue_id: None,
            config_id: None,
        })
        .await?;
    println!("Created numeric score: {}", numeric.id);

    // Create a categorical score
    let categorical = client
        .scores
        .create(&CreateScoreRequest {
            name: "sentiment".into(),
            value: CreateScoreValue::String("positive".into()),
            trace_id: Some("example-trace-id".into()),
            data_type: Some(ScoreDataType::Categorical),
            comment: Some("User feedback was positive".into()),
            id: None,
            session_id: None,
            observation_id: None,
            dataset_run_id: None,
            metadata: None,
            environment: None,
            queue_id: None,
            config_id: None,
        })
        .await?;
    println!("Created categorical score: {}", categorical.id);

    Ok(())
}
