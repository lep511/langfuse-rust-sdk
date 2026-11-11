//! Organization overview: projects with their members, organization members,
//! organization API keys and the status of the blob storage exports.
//!
//! **Read-only:** writes no data.
//!
//! **Needs:** an **organization-scoped** API key instead of the usual project
//! key. Organization owners create one in the Langfuse UI under Organization
//! settings → API keys; on Langfuse Cloud these keys are part of the
//! Enterprise plan. With a project key the API answers 403 and the example
//! explains what is missing and exits successfully.
//!
//! **Docs:** <https://langfuse.com/docs/administration/scim-and-org-api>,
//! <https://langfuse.com/docs/api-and-data-platform/features/export-to-blob-storage>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-<org key> LANGFUSE_SECRET_KEY=sk-lf-<org key> \
//!   cargo run --example organization_admin
//! ```

use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // The first call tells whether the key is organization-scoped: a project
    // key is rejected with 401 or 403.
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

    // Every project of the organization, with the role of each member in it.
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

    // Members and their organization-level roles.
    let members = client.organizations.get_memberships().await?.memberships;
    println!("\n=== Organization members ({}) ===", members.len());
    for m in &members {
        println!("  {:<28} {:<20} {:?}", m.email, m.name, m.role);
    }

    // The organization's own API keys. Of the secret key only a masked form
    // is returned.
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

    // Scheduled exports of project data to S3, GCS or Azure Blob Storage.
    // Each integration belongs to one project; its status is a second call.
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
