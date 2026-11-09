//! Inspect the SCIM 2.0 provisioning endpoint: supported features, resource
//! types, and every organization user (SCIM pagination with `startIndex`).
//! Read-only.
//!
//! Requires an **organization-scoped** API key. Set `SCIM_USER_EMAIL` to look up
//! a single user with a SCIM filter instead of listing everyone.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-<org key> LANGFUSE_SECRET_KEY=sk-lf-<org key> \
//!   cargo run --example scim_users
//! ```

use langfuse_sdk::api::scim::ListScimUsersParams;
use langfuse_sdk::{ClientConfig, LangfuseClient, LangfuseError};

const PAGE_SIZE: i64 = 50;

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

    let config = match client.scim.get_service_provider_config().await {
        Ok(config) => config,
        Err(err @ (LangfuseError::Unauthorized { .. } | LangfuseError::AccessDenied { .. })) => {
            println!(
                "SCIM needs an organization-scoped API key; the current key was rejected ({err})."
            );
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    println!("=== Service provider config ===");
    println!(
        "  filter: {} (max {} results), patch: {}, bulk: {}, sort: {}",
        config.filter.supported,
        config.filter.max_results,
        config.patch.supported,
        config.bulk.supported,
        config.sort.supported
    );
    for scheme in &config.authentication_schemes {
        println!("  auth: {} ({})", scheme.name, scheme.r#type);
    }

    let resource_types = client.scim.get_resource_types().await?;
    println!(
        "\n=== Resource types ({}) ===",
        resource_types.total_results
    );
    for rt in &resource_types.resources {
        println!("  {} at {} (schema {})", rt.name, rt.endpoint, rt.schema);
    }

    // Users: either one filtered lookup or all pages. SCIM indexes start at 1.
    let filter = std::env::var("SCIM_USER_EMAIL")
        .ok()
        .map(|email| format!("userName eq \"{email}\""));
    let mut start_index = 1;
    let mut users = Vec::new();
    loop {
        let page = client
            .scim
            .list_users(&ListScimUsersParams {
                filter: filter.clone(),
                start_index: Some(start_index),
                count: Some(PAGE_SIZE),
            })
            .await?;
        let fetched = page.resources.len() as i64;
        users.extend(page.resources);
        start_index += fetched;
        if fetched == 0 || start_index > page.total_results {
            break;
        }
    }

    println!("\n=== Users ({}) ===", users.len());
    for user in &users {
        let primary = user
            .emails
            .iter()
            .find(|e| e.primary)
            .map(|e| e.value.as_str())
            .unwrap_or("-");
        println!(
            "  {:<36} {:<28} {} (created {})",
            user.id,
            user.user_name,
            user.name.formatted.as_deref().unwrap_or(primary),
            user.meta.created.as_deref().unwrap_or("-"),
        );
    }

    Ok(())
}
