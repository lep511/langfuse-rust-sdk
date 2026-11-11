//! List the project's LLM connections, and the organization-level ones when
//! the key allows it. An LLM connection is a provider API key stored in
//! Langfuse, which the Playground and LLM-as-a-judge evaluators use to call
//! models; it has no effect on the cost calculation of ingested traces. The
//! API never returns the secrets, only a masked key and the names of extra
//! headers.
//!
//! **Read-only** by default. Set `EXAMPLE_LLM_API_KEY` to also upsert a
//! temporary OpenAI-compatible connection named `rust-sdk-example`, show it,
//! and delete it again (**writes data**, then cleans up). The upsert does not
//! check the key with the provider, so a dummy value is enough to try it.
//! Optionally set `EXAMPLE_LLM_BASE_URL` to the base URL of an
//! OpenAI-compatible server.
//!
//! Organization-level connections need an organization-scoped API key and are
//! an Enterprise feature; with a project key that part is skipped.
//!
//! **Docs:** <https://langfuse.com/docs/administration/llm-connection>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example llm_connections
//! ```

use langfuse_rust_sdk::models::llm_connections::{
    LlmAdapter, LlmConnection, UpsertLlmConnectionRequest,
};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

const EXAMPLE_PROVIDER: &str = "rust-sdk-example";

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// One line per connection; the secret key is not part of the response.
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

    // Project-level connections: walk through all pages by hand
    // (`llm_connections.list_pages` does the same lazily).
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

    // Organization-level connections are shared by the projects of the
    // organization. They need an organization-scoped API key: a project key
    // is answered with 401 or 403, which is not a failure of this example.
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

    // Optional write path. Connections are upserted on the provider name:
    // a second call with the same `provider` updates the connection instead
    // of adding one. The adapter is the API format the provider speaks.
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

    // Deleting a connection pauses the evaluators that depend on it.
    let deleted = client.llm_connections.delete(&created.id).await?;
    println!("Deleted: {}", deleted.message);

    Ok(())
}
