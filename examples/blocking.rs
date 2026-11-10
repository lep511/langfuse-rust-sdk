//! The blocking client from a plain `main` (no async runtime): health, project,
//! the first prompts and datasets, and a view with per-request options that
//! prints the rate-limit headers of each response.
//!
//! Read-only: writes no data.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example blocking --features blocking
//! ```

use std::time::Duration;

use langfuse_rust_sdk::blocking::LangfuseClient;
use langfuse_rust_sdk::blocking::api::prompts::ListPromptsParams;
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, RequestOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    let health = client.health.check()?;
    println!("Langfuse {} is {}", health.version, health.status);
    let project = client.auth_check()?;
    println!("Project: {} ({})", project.name, project.id);

    // A view of the client: short timeout, no retries, an extra header and a
    // hook that sees every response.
    let quick = client.with_options(
        RequestOptions::default()
            .with_timeout(Duration::from_secs(10))
            .with_max_retries(0)
            .with_header("x-request-source", "rust-sdk-example")
            .with_response_hook(|response| {
                let remaining = response
                    .headers
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("-");
                println!(
                    "  {} {} -> {} (rate limit remaining: {remaining})",
                    response.method,
                    response.url.path(),
                    response.status
                );
            }),
    );
    let params = ListPromptsParams {
        limit: Some(5),
        ..Default::default()
    };
    let prompts = quick.prompts.list(&params)?;
    println!("{} prompts in total; first ones:", prompts.meta.total_items);
    for prompt in &prompts.data {
        println!("  {}", prompt.name);
    }

    // Iterators fetch one page per step.
    let params = PageParams {
        limit: Some(20),
        ..Default::default()
    };
    println!("Datasets:");
    for dataset in client.datasets.list_pages(params).items().take(5) {
        println!("  {}", dataset?.name);
    }
    client.shutdown();
    Ok(())
}
