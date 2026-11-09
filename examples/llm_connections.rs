//! List the project's LLM connections (secrets are never returned), and the
//! organization-level ones when the key allows it.
//!
//! Read-only by default. Set `EXAMPLE_LLM_API_KEY` to also upsert a temporary
//! OpenAI-compatible connection named `rust-sdk-example`, show it, and delete it.
//! Optionally set `EXAMPLE_LLM_BASE_URL` for a custom gateway.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example llm_connections
//! ```

use langfuse_sdk::models::llm_connections::{
    LlmAdapter, LlmConnection, UpsertLlmConnectionRequest,
};
use langfuse_sdk::pagination::PageParams;
use langfuse_sdk::{ClientConfig, LangfuseClient, LangfuseError};

const EXAMPLE_PROVIDER: &str = "rust-sdk-example";

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

fn print_connection(conn: &LlmConnection) {
    println!(
        "  - {} (adapter {}, base URL {}, default models: {})",
        conn.provider,
        conn.adapter,
        conn.base_url.as_deref().unwrap_or("default"),
        conn.with_default_models
    );
    if !conn.custom_models.is_empty() {
        println!("      custom models: {}", conn.custom_models.join(", "));
    }
    if !conn.extra_header_keys.is_empty() {
        println!("      extra headers: {}", conn.extra_header_keys.join(", "));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Project-level connections, all pages.
    let mut page = 1;
    let mut connections = Vec::new();
    loop {
        let resp = client
            .llm_connections
            .list(&PageParams {
                page: Some(page),
                limit: Some(50),
            })
            .await?;
        connections.extend(resp.data);
        if page >= resp.meta.total_pages {
            break;
        }
        page += 1;
    }
    println!("Project LLM connections: {}", connections.len());
    connections.iter().for_each(print_connection);

    // Organization-level connections need an organization-scoped API key.
    match client
        .llm_connections
        .list_organization(&PageParams::default())
        .await
    {
        Ok(resp) => {
            println!("\nOrganization LLM connections: {}", resp.data.len());
            resp.data.iter().for_each(print_connection);
        }
        Err(LangfuseError::Unauthorized { .. } | LangfuseError::AccessDenied { .. }) => {
            println!("\nOrganization connections: skipped (needs an organization-scoped key)");
        }
        Err(e) => return Err(e.into()),
    }

    // Optional write path: upsert a temporary connection and delete it again.
    let Ok(secret) = std::env::var("EXAMPLE_LLM_API_KEY") else {
        println!("\nSet EXAMPLE_LLM_API_KEY to also try upsert + delete.");
        return Ok(());
    };
    let created = client
        .llm_connections
        .upsert(&UpsertLlmConnectionRequest {
            base_url: std::env::var("EXAMPLE_LLM_BASE_URL").ok(),
            custom_models: Some(vec!["my-fine-tuned-model".into()]),
            with_default_models: Some(true),
            ..UpsertLlmConnectionRequest::new(EXAMPLE_PROVIDER, LlmAdapter::Openai, secret)
        })
        .await?;
    println!("\nUpserted connection:");
    print_connection(&created);

    let deleted = client.llm_connections.delete(&created.id).await?;
    println!("Deleted: {}", deleted.message);

    Ok(())
}
