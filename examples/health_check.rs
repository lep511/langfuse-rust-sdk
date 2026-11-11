//! Smoke test for a new setup: check that the Langfuse server is healthy,
//! verify the API keys and print the project they belong to.
//!
//! **Read-only:** writes no data.
//!
//! The health endpoint needs no credentials, so `health.check()` succeeds even
//! with wrong keys; `auth_check()` is the call that proves the keys work.
//!
//! **Docs:** <https://langfuse.com/docs/api-and-data-platform/features/public-api>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example health_check
//! ```

use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load `.env` from the working directory if there is one; variables that
    // are already set in the environment win.
    let _ = dotenvy::dotenv();

    // Reads LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY, LANGFUSE_BASE_URL
    // (default: https://cloud.langfuse.com, the EU region of Langfuse Cloud)
    // and LANGFUSE_TIMEOUT.
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // `GET /api/public/health`: unauthenticated, reports status and version.
    let health = client.health.check().await?;
    println!("Langfuse server is healthy!");
    println!("  Status:  {}", health.status);
    println!("  Version: {}", health.version);

    // `GET /api/public/projects`: a project-scoped key returns its own project.
    // Fails with `LangfuseError::Unauthorized` when the keys are wrong.
    let project = client.auth_check().await?;
    println!("API keys are valid for project '{}'", project.name);
    println!("  Project:      {}", project.id);
    println!("  Organization: {}", project.organization.name);

    Ok(())
}
