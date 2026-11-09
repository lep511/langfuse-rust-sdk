//! Create and list score configurations.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example score_configs
//! ```

use langfuse_sdk::models::enums::ScoreConfigDataType;
use langfuse_sdk::models::score_configs::{CreateConfigCategory, CreateScoreConfigRequest};
use langfuse_sdk::pagination::PageParams;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Create a numeric score config with min/max range
    let numeric_config = client
        .score_configs
        .create(&CreateScoreConfigRequest {
            name: "relevance-score".into(),
            data_type: ScoreConfigDataType::Numeric,
            min_value: Some(0.0),
            max_value: Some(1.0),
            description: Some("How relevant is the response to the query (0-1)".into()),
            categories: None,
        })
        .await?;
    println!(
        "Created numeric config: {} ({})",
        numeric_config.name, numeric_config.id
    );
    println!(
        "  Range: [{}, {}]",
        numeric_config.min_value.unwrap_or(0.0),
        numeric_config.max_value.unwrap_or(0.0)
    );

    // Create a categorical score config
    let cat_config = client
        .score_configs
        .create(&CreateScoreConfigRequest {
            name: "response-quality".into(),
            data_type: ScoreConfigDataType::Categorical,
            categories: Some(vec![
                CreateConfigCategory {
                    value: 3.0,
                    label: "excellent".into(),
                },
                CreateConfigCategory {
                    value: 2.0,
                    label: "good".into(),
                },
                CreateConfigCategory {
                    value: 1.0,
                    label: "acceptable".into(),
                },
                CreateConfigCategory {
                    value: 0.0,
                    label: "poor".into(),
                },
            ]),
            min_value: None,
            max_value: None,
            description: Some("Overall response quality assessment".into()),
        })
        .await?;
    println!(
        "\nCreated categorical config: {} ({})",
        cat_config.name, cat_config.id
    );

    // List all score configs
    let configs = client
        .score_configs
        .list(&PageParams {
            page: Some(1),
            limit: Some(20),
        })
        .await?;
    println!(
        "\nAll score configs: {} of {}",
        configs.data.len(),
        configs.meta.total_items
    );
    for cfg in &configs.data {
        let archived = if cfg.is_archived { " [archived]" } else { "" };
        println!(
            "  - {} ({:?}){} — {}",
            cfg.name, cfg.data_type, archived, cfg.id
        );
    }

    Ok(())
}
