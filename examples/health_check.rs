//! Check Langfuse server health, verify the API keys and print the project.
//!
//! Read-only: writes no data.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example health_check
//! ```

use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    // Reads LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY, LANGFUSE_BASE_URL and LANGFUSE_TIMEOUT.
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    let health = client.health.check().await?;
    println!("Langfuse server is healthy!");
    println!("  Status:  {}", health.status);
    println!("  Version: {}", health.version);

    let project = client.auth_check().await?;
    println!("API keys are valid for project '{}'", project.name);
    println!("  Project:      {}", project.id);
    println!("  Organization: {}", project.organization.name);

    Ok(())
}
