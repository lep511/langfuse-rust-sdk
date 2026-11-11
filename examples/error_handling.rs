//! Error handling patterns: every API method returns
//! `Result<T, LangfuseError>`, and each HTTP status has its own variant, so
//! callers can `match` on the failure they expect and propagate the rest.
//!
//! The example triggers, on purpose: a configuration error (no request is
//! sent), two 404s (`NotFound`), a 401 (`Unauthorized`) with invalid keys, and
//! a "get or create" fallback on a missing dataset.
//!
//! **Read-only:** writes no data (all requests are `GET`s that fail).
//!
//! `LangfuseError` is `#[non_exhaustive]`: keep a catch-all arm. The other
//! status variants are `BadRequest` (400), `AccessDenied` (403), `Conflict`
//! (409), `PreconditionFailed` (412), `RateLimited` (429),
//! `ServiceUnavailable` (503), `ClientError` (other 4xx) and `ServerError`
//! (other 5xx). 408, 429, 503 and connection failures are retried by the SDK
//! before the error reaches the caller.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example error_handling
//! ```

use langfuse_rust_sdk::api::skills::GetSkillParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // 1. Configuration error: the base URL is validated by `build()`, before
    //    any request is made.
    println!("=== Config Error ===");
    match ClientConfig::builder("pk", "sk").base_url("://bad").build() {
        Err(LangfuseError::Config(msg)) => println!("  Config error caught: {msg}"),
        other => println!("  Unexpected: {other:?}"),
    }

    // 2. Not found: a skill that does not exist. `message` is the server's
    //    `message` (or `error`) field; the raw JSON body is in `body`.
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

    // 3. Not found: a prompt that does not exist. Without a label or version
    //    the API looks for the `production` label, as the message shows.
    println!("\n=== Prompt Not Found ===");
    match client.prompts.get("nonexistent-prompt-xyz").await {
        Err(LangfuseError::NotFound { message, .. }) => {
            println!("  Prompt not found (expected): {message}");
        }
        Err(e) => println!("  Other error: {e}"),
        Ok(_) => println!("  Unexpectedly found"),
    }

    // 4. Unauthorized: a client with invalid keys. `auth_check` calls an
    //    authenticated endpoint; the health endpoint would not do, because it
    //    answers without credentials.
    println!("\n=== Unauthorized ===");
    let bad_config = ClientConfig::builder("pk-invalid", "sk-invalid")
        .base_url(
            std::env::var("LANGFUSE_BASE_URL")
                .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
        )
        .build()?;
    let bad_client = LangfuseClient::new(bad_config)?;
    match bad_client.auth_check().await {
        Err(LangfuseError::Unauthorized { message, .. }) => {
            println!("  Unauthorized (expected): {message}");
        }
        Err(e) => println!("  Other error: {e}"),
        Ok(project) => println!("  Unexpectedly authenticated for '{}'", project.name),
    }

    // 5. Graceful fallback: treat `NotFound` as a normal outcome (here one
    //    could create the dataset) and propagate everything else with `?`.
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
