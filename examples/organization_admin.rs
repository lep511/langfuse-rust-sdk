//! Organization overview: projects with their members, organization members,
//! organization API keys and blob storage export status. Read-only.
//!
//! Requires an **organization-scoped** API key (create one in the Langfuse UI
//! under Organization settings → API keys). With a project key the example
//! explains what is missing and exits.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-<org key> LANGFUSE_SECRET_KEY=sk-lf-<org key> \
//!   cargo run --example organization_admin
//! ```

use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

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

    let projects = match client.organizations.get_projects().await {
        Ok(resp) => resp.projects,
        Err(err @ (LangfuseError::Unauthorized { .. } | LangfuseError::AccessDenied { .. })) => {
            println!(
                "This example needs an organization-scoped API key; the current key was rejected ({err})."
            );
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    println!("=== Projects ({}) ===", projects.len());
    for project in &projects {
        println!("- {} ({})", project.name, project.id);
        let members = client
            .organizations
            .get_project_memberships(&project.id)
            .await?
            .memberships;
        for m in &members {
            println!("    {:<28} {:?}", m.email, m.role);
        }
    }

    let members = client.organizations.get_memberships().await?.memberships;
    println!("\n=== Organization members ({}) ===", members.len());
    for m in &members {
        println!("  {:<28} {:<20} {:?}", m.email, m.name, m.role);
    }

    let keys = client.organizations.get_api_keys().await?.api_keys;
    println!("\n=== Organization API keys ({}) ===", keys.len());
    for key in &keys {
        let last_used = key
            .last_used_at
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "never".into());
        let expires = key
            .expires_at
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "never".into());
        println!(
            "  {} ({}) last used {last_used}, expires {expires}",
            key.public_key,
            key.name.as_deref().unwrap_or("unnamed"),
        );
    }

    let integrations = client.blob_storage_integrations.list().await?.data;
    println!("\n=== Blob storage exports ({}) ===", integrations.len());
    for integration in &integrations {
        let status = client
            .blob_storage_integrations
            .get_status(&integration.id)
            .await?;
        println!(
            "  project {}: {:?} bucket {} (prefix '{}') every {:?}, status {:?}, last sync {}",
            integration.project_id,
            integration.r#type,
            integration.bucket_name,
            integration.prefix,
            integration.export_frequency,
            status.sync_status,
            status
                .last_sync_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "never".into()),
        );
        if let Some(error) = &status.last_error {
            println!("    last error: {error}");
        }
    }

    Ok(())
}
