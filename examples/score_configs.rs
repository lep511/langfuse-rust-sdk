//! Create a numeric and a categorical score config, then list the project's
//! configs. A score config is the schema of a score: its data type, the value
//! range of a numeric score or the allowed categories of a categorical one.
//! Scores that reference a config (`config_id`) are validated against it, and
//! annotation queues are built from configs.
//!
//! **Writes data:** every run adds two configs, `relevance-score` and
//! `response-quality` (names do not have to be unique). Score configs cannot
//! be deleted, only archived: in the UI, or with
//! `client.score_configs.update(config_id, ..)` setting `is_archived`.
//!
//! **Docs:** <https://langfuse.com/docs/evaluation/scores/data-model>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example score_configs
//! ```

use langfuse_rust_sdk::models::enums::ScoreConfigDataType;
use langfuse_rust_sdk::models::score_configs::{CreateConfigCategory, CreateScoreConfigRequest};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Numeric: an optional range. A score that references the config must
    // have the config's name and a value inside the range.
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

    // Categorical: the allowed labels, each mapped to a number so that the
    // score can be aggregated. `min_value` and `max_value` do not apply.
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

    // Page-based pagination: `meta` has the totals. Archived configs are
    // listed too, flagged with `is_archived`.
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
