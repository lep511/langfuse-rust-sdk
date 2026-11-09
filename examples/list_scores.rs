//! List scores using cursor-based pagination.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example list_scores
//! ```

use langfuse_sdk::api::scores::ListScoresParams;
use langfuse_sdk::models::scores::ScoreV3;
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

    let mut cursor: Option<String> = None;
    let mut total = 0;

    loop {
        let page = client
            .scores
            .list(&ListScoresParams {
                limit: Some(10),
                cursor: cursor.clone(),
                ..Default::default()
            })
            .await?;

        for score in &page.data {
            total += 1;
            println!("  [{total}] {}", score_summary(score));
        }

        match page.meta.cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    println!("\nTotal scores fetched: {total}");
    Ok(())
}
