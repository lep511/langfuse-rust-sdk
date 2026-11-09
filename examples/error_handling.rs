//! Demonstrates error handling patterns with the SDK.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example error_handling
//! ```

use langfuse_sdk::api::skills::GetSkillParams;
use langfuse_sdk::{ClientConfig, LangfuseClient, LangfuseError};

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

    // 1. Configuration error: invalid URL
    println!("=== Config Error ===");
    match ClientConfig::builder("pk", "sk").base_url("://bad").build() {
        Err(LangfuseError::Config(msg)) => println!("  Config error caught: {msg}"),
        other => println!("  Unexpected: {other:?}"),
    }

    // 2. Not Found: try to fetch a skill that doesn't exist
    println!("\n=== Not Found ===");
    match client
        .skills
        .get("nonexistent-skill-xyz-12345", &GetSkillParams::default())
        .await
    {
        Err(LangfuseError::NotFound { message, .. }) => {
            println!("  Skill not found (expected): {message}");
        }
        Err(e) => println!("  Other error: {e}"),
        Ok(skill) => println!("  Unexpectedly found: {}", skill.name),
    }

    // 3. Not Found: try to fetch a nonexistent prompt
    println!("\n=== Prompt Not Found ===");
    match client.prompts.get("nonexistent-prompt-xyz").await {
        Err(LangfuseError::NotFound { message, .. }) => {
            println!("  Prompt not found (expected): {message}");
        }
        Err(e) => println!("  Other error: {e}"),
        Ok(_) => println!("  Unexpectedly found"),
    }

    // 4. Bad credentials: create a client with wrong keys
    println!("\n=== Unauthorized ===");
    let bad_config = ClientConfig::builder("pk-invalid", "sk-invalid")
        .base_url(
            std::env::var("LANGFUSE_BASE_URL")
                .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
        )
        .build()?;
    let bad_client = LangfuseClient::new(bad_config)?;
    match bad_client.health.check().await {
        Ok(h) => println!(
            "  Health returned (server may not require auth): {}",
            h.status
        ),
        Err(LangfuseError::Unauthorized { message, .. }) => {
            println!("  Unauthorized (expected): {message}");
        }
        Err(e) => println!("  Other error: {e}"),
    }

    // 5. Pattern: graceful fallback with match
    println!("\n=== Graceful Fallback ===");
    let result = client.datasets.get("maybe-exists").await;
    match result {
        Ok(ds) => println!("  Found dataset: {}", ds.name),
        Err(LangfuseError::NotFound { .. }) => {
            println!("  Dataset not found — could create it here");
        }
        Err(e) => {
            println!("  Unexpected error, propagating: {e}");
            return Err(e.into());
        }
    }

    println!("\nAll error scenarios handled successfully!");
    Ok(())
}
