//! Create a dataset, upsert three items (stable IDs, so reruns update them
//! instead of adding more), and fetch the dataset with all its items. Writes a
//! dataset named `rust-sdk-example`; the API cannot delete datasets.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example manage_datasets
//! ```

use langfuse_rust_sdk::api::datasets::GetDatasetOptions;
use langfuse_rust_sdk::models::dataset_items::CreateDatasetItemRequest;
use langfuse_rust_sdk::models::datasets::CreateDatasetRequest;
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
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
    let items = [
        ("What is Rust?", "Rust is a systems programming language."),
        (
            "What is Langfuse?",
            "Langfuse is an LLM observability platform.",
        ),
        ("What is tokio?", "Tokio is an async runtime for Rust."),
    ];

    for (index, (input, expected)) in items.iter().enumerate() {
        // `create_dataset_item` also uploads media found in the item (see the `media` module).
        let item = client
            .create_dataset_item(&CreateDatasetItemRequest {
                id: Some(format!("rust-sdk-example-{index}")),
                input: Some(serde_json::json!({"question": input})),
                expected_output: Some(serde_json::json!({"answer": expected})),
                ..CreateDatasetItemRequest::new(&dataset.name)
            })
            .await?;
        println!("  Upserted item: {}", item.id);
    }

    // Fetch the dataset back with every item.
    let fetched = client
        .get_dataset(&dataset.name, &GetDatasetOptions::default())
        .await?;
    println!(
        "\nFetched dataset: {} ({} items)",
        fetched.name,
        fetched.items.len()
    );
    println!("  Description: {:?}", fetched.description);
    for item in &fetched.items {
        println!("  - {}: {:?}", item.id, item.input);
    }

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
