//! Register custom model definitions with pricing information.
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

    // Register a custom model with pricing
    let model = client
        .models
        .create(&CreateModelRequest {
            input_price: Some(0.003),
            output_price: Some(0.012),
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

    // List all models
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

    // Clean up: delete the model we created
    client.models.delete(&model.id).await?;
    println!("\nDeleted model: {}", model.id);

    Ok(())
}
