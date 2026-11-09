//! List scores with the lazy cursor-pagination stream (`scores.list_pages`).
//!
//! Read-only: writes no data. Stops after `MAX_SCORES` scores.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example list_scores
//! ```

use futures::{StreamExt, TryStreamExt};
use langfuse_rust_sdk::api::scores::ListScoresParams;
use langfuse_rust_sdk::models::scores::ScoreV3;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

/// Cap on fetched scores: no page beyond the one holding the last of them is requested.
const MAX_SCORES: usize = 100;

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

fn score_summary(score: &ScoreV3) -> String {
    match score {
        ScoreV3::Numeric { base, value } => {
            format!("{} = {:.2} (numeric)", base.name, value)
        }
        ScoreV3::Boolean { base, value } => {
            format!("{} = {} (boolean)", base.name, value)
        }
        ScoreV3::Categorical { base, value } => {
            format!("{} = {} (categorical)", base.name, value)
        }
        ScoreV3::Correction { base, value } => {
            format!("{} = {} (correction)", base.name, value)
        }
        ScoreV3::Text { base, value } => {
            let preview: String = value.chars().take(40).collect();
            format!("{} = \"{}\" (text)", base.name, preview)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    let params = ListScoresParams {
        limit: Some(10),
        ..Default::default()
    };
    // Pages are requested lazily, following `meta.cursor`.
    let mut scores = client.scores.list_pages(params).items().take(MAX_SCORES);

    let mut total = 0;
    while let Some(score) = scores.try_next().await? {
        total += 1;
        println!("  [{total}] {}", score_summary(&score));
    }

    println!("\nTotal scores fetched: {total}");
    Ok(())
}
