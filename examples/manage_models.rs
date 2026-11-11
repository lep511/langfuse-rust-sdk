//! Register a custom model definition with prices, list the project's model
//! definitions, and delete the new one again. Langfuse uses model definitions
//! to compute the cost of generations: the model name of a generation is
//! matched against `match_pattern`, and its usage is multiplied by the prices.
//!
//! **Writes data**, then cleans up: the model `my-fine-tuned-llm` is deleted
//! at the end.
//!
//! Custom definitions take priority over the ones Langfuse maintains, and a
//! new or changed definition only applies to generations ingested afterwards:
//! costs of existing generations are not recalculated.
//!
//! **Docs:** <https://langfuse.com/docs/observability/features/token-and-cost-tracking>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example manage_models
//! ```

use langfuse_rust_sdk::models::enums::ModelUsageUnit;
use langfuse_rust_sdk::models::models_api::CreateModelRequest;
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // `new(model_name, match_pattern, unit)`: the pattern is a regular
    // expression, here case-insensitive and matching every model name that
    // starts with `my-fine-tuned-llm`. Prices are in USD per unit, so per
    // token: $3 and $12 per million tokens.
    //
    // `input_price` and `output_price` are the simple, legacy way to set
    // prices (they create one default pricing tier); use `pricing_tiers` when
    // the price depends on the usage, e.g. on the context size.
    let model = client
        .models
        .create(&CreateModelRequest {
            input_price: Some(0.000_003),
            output_price: Some(0.000_012),
            ..CreateModelRequest::new(
                "my-fine-tuned-llm",
                "(?i)^my-fine-tuned-llm.*$",
                ModelUsageUnit::Tokens,
            )
        })
        .await?;
    println!("Registered model: {} ({})", model.model_name, model.id);
    println!("  Match pattern: {}", model.match_pattern);
    println!("  Unit: {:?}", model.unit);
    println!("  Input price: {:?}", model.input_price);
    println!("  Output price: {:?}", model.output_price);

    // The list holds the project's own definitions and the ones maintained by
    // Langfuse (a few hundred), page by page.
    let models = client
        .models
        .list(&PageParams {
            page: Some(1),
            limit: Some(20),
        })
        .await?;
    println!(
        "\nAll models: {} of {} total",
        models.data.len(),
        models.meta.total_items
    );
    for m in &models.data {
        println!("  - {} ({:?}) — {}", m.model_name, m.unit, m.id);
    }

    // Clean up. Only custom definitions can be deleted, not the ones
    // maintained by Langfuse.
    client.models.delete(&model.id).await?;
    println!("\nDeleted model: {}", model.id);

    Ok(())
}
