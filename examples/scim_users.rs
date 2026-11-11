//! Inspect the SCIM 2.0 endpoints (`/api/public/scim`), which identity
//! providers use to provision the users of an organization: supported
//! features, resource types, and every organization user (SCIM pagination
//! with `startIndex`).
//!
//! **Read-only:** writes no data.
//!
//! **Needs:** an **organization-scoped** API key (on Langfuse Cloud part of
//! the Enterprise plan). With a project key the API answers 403 and the
//! example explains what is missing and exits successfully.
//!
//! Set `SCIM_USER_EMAIL` to look up a single user with a SCIM filter instead
//! of listing everyone.
//!
//! **Docs:** <https://langfuse.com/docs/administration/scim-and-org-api>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-<org key> LANGFUSE_SECRET_KEY=sk-lf-<org key> \
//!   cargo run --example scim_users
//! ```

use langfuse_rust_sdk::api::scim::ListScimUsersParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

const PAGE_SIZE: i64 = 50;

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // `GET /ServiceProviderConfig`: what the SCIM implementation supports. It
    // is also the call that shows whether the key is organization-scoped.
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

    // `GET /ResourceTypes`: the kinds of resources that can be provisioned.
    let resource_types = client.scim.get_resource_types().await?;
    println!(
        "\n=== Resource types ({}) ===",
        resource_types.total_results
    );
    for rt in &resource_types.resources {
        println!("  {} at {} (schema {})", rt.name, rt.endpoint, rt.schema);
    }

    // Users: either one filtered lookup or all pages. SCIM pagination is
    // neither page- nor cursor-based: `startIndex` is the 1-based position of
    // the first result and `count` the page size; `totalResults` tells when
    // to stop.
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
