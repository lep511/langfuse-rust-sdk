//! Create a dataset, upsert three items into it, fetch the dataset back with
//! all of its items, and list the project's datasets. Datasets hold the test
//! cases (input and expected output) that experiments run on.
//!
//! **Writes data:** the dataset `rust-sdk-example` with three items, kept.
//! Running the example again changes nothing: dataset names are unique in a
//! project (creating an existing name does not add a second dataset) and the
//! items have fixed IDs, so they are updated instead of duplicated. The
//! public API cannot delete a dataset; remove it in the UI, or delete single
//! items with `client.dataset_items.delete(item_id)`.
//!
//! To run an experiment on the dataset, see the `run_experiment` example.
//!
//! **Docs:** <https://langfuse.com/docs/evaluation/experiments/datasets>
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

    // `input_schema` and `expected_output_schema` could hold JSON Schemas
    // that every item is validated against. A `/` in the name would put the
    // dataset into a folder in the UI.
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

    // Three question/answer pairs.
    let items = [
        ("What is Rust?", "Rust is a systems programming language."),
        (
            "What is Langfuse?",
            "Langfuse is an LLM observability platform.",
        ),
        ("What is tokio?", "Tokio is an async runtime for Rust."),
    ];

    for (index, (input, expected)) in items.iter().enumerate() {
        // Items are upserted on their ID, which is unique in the project (it
        // cannot be reused in another dataset); without an ID every call adds
        // a new item. `client.create_dataset_item` is the high-level variant
        // of `client.dataset_items.create`: it first uploads the media found
        // in the item (see the `media` example).
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

    // `get_dataset` reads the dataset and then every page of its items;
    // `client.datasets.get` returns the dataset alone.
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

    // Page-based pagination: `meta` has the totals, and
    // `datasets.list_pages` would walk through every page.
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
