//! Check Langfuse server health and print the version.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example health_check
//! ```

use langfuse_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;

    let client = LangfuseClient::new(config)?;
    let health = client.health.check().await?;

    println!("Langfuse server is healthy!");
    println!("  Status:  {}", health.status);
    println!("  Version: {}", health.version);

    Ok(())
}
