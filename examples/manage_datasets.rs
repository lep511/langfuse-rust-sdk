//! Create a dataset and add items to it.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example manage_datasets
//! ```

use langfuse_sdk::models::dataset_items::CreateDatasetItemRequest;
use langfuse_sdk::models::datasets::CreateDatasetRequest;
use langfuse_sdk::pagination::PageParams;
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

    // Create a dataset
    let dataset = client
        .datasets
        .create(&CreateDatasetRequest {
            name: "rust-sdk-example".into(),
            description: Some("Dataset created from the Rust SDK example".into()),
            metadata: Some(serde_json::json!({"source": "rust-sdk", "version": "0.1.0"})),
            input_schema: None,
            expected_output_schema: None,
        })
        .await?;
    println!("Created dataset: {} ({})", dataset.name, dataset.id);

    // Add items to the dataset
    let items = vec![
        ("What is Rust?", "Rust is a systems programming language."),
        ("What is Langfuse?", "Langfuse is an LLM observability platform."),
        ("What is tokio?", "Tokio is an async runtime for Rust."),
    ];

    for (input, expected) in &items {
        let item = client
            .dataset_items
            .create(&CreateDatasetItemRequest {
                dataset_name: dataset.name.clone(),
                input: Some(serde_json::json!({"question": input})),
                expected_output: Some(serde_json::json!({"answer": expected})),
                metadata: None,
                source_trace_id: None,
                source_observation_id: None,
                id: None,
                status: None,
            })
            .await?;
        println!("  Added item: {}", item.id);
    }

    // Fetch the dataset back
    let fetched = client.datasets.get(&dataset.name).await?;
    println!("\nFetched dataset: {}", fetched.name);
    println!("  Description: {:?}", fetched.description);

    // List all datasets
    let all = client
        .datasets
        .list(&PageParams {
            page: Some(1),
            limit: Some(5),
        })
        .await?;
    println!(
        "\nAll datasets (page 1): {} of {}",
        all.data.len(),
        all.meta.total_items
    );
    for ds in &all.data {
        println!("  - {} ({})", ds.name, ds.id);
    }

    Ok(())
}
