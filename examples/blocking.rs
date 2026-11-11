//! The blocking client from a plain `main` (no `#[tokio::main]`, no `.await`):
//! health, project, the first prompts and datasets, and a view of the client
//! with per-request options whose hook logs every response.
//!
//! **Read-only:** writes no data.
//!
//! **Needs:** the `blocking` feature. `blocking::LangfuseClient` mirrors the
//! async client method by method and runs them on a runtime of its own, so do
//! not call it from inside an async runtime.
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
    // Same configuration as the async client (`ClientConfig::from_env`).
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    let health = client.health.check()?;
    println!("Langfuse {} is {}", health.version, health.status);
    let project = client.auth_check()?;
    println!("Project: {} ({})", project.name, project.id);

    // A view of the client: every request made through `quick` gets a short
    // timeout, no retries and an extra header; `client` itself is unchanged.
    // The hook sees each response before its body is decoded. Langfuse
    // reports rate limits only on a 429, with a `Retry-After` header; with
    // retries disabled that response reaches the hook and then the caller as
    // `LangfuseError::RateLimited`.
    let quick = client.with_options(
        RequestOptions::default()
            .with_timeout(Duration::from_secs(10))
            .with_max_retries(0)
            .with_header("x-request-source", "rust-sdk-example")
            .with_response_hook(|response| {
                let retry_after = response
                    .headers
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .map(|secs| format!(", retry after {secs} s"))
                    .unwrap_or_default();
                println!(
                    "  {} {} -> {} (attempt {}{retry_after})",
                    response.method,
                    response.url.path(),
                    response.status,
                    response.attempt + 1
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

    // `list_pages(..).items()` is a plain `Iterator` over all items: it
    // requests one page at a time and only when the previous one is used up,
    // so `take(5)` never fetches more than the first page here.
    let params = PageParams {
        limit: Some(20),
        ..Default::default()
    };
    println!("Datasets:");
    for dataset in client.datasets.list_pages(params).items().take(5) {
        println!("  {}", dataset?.name);
    }

    // Waits for queued scores and media uploads (none here) before exiting.
    client.shutdown();
    Ok(())
}
