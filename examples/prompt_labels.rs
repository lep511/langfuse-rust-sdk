//! Release management with prompt labels: create two versions of a prompt in a
//! folder, promote version 1 to `production`, fetch by label and by version,
//! find the prompt with a structured filter, then delete it.
//!
//! **Writes data**, then cleans up: the prompt `rust-sdk-examples/label-demo`
//! is deleted at the end (all of its versions).
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example prompt_labels
//! ```

use langfuse_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_sdk::api::prompts::{DeletePromptParams, GetPromptParams, ListPromptsParams};
use langfuse_sdk::models::prompts::{CreatePromptRequest, Prompt};
use langfuse_sdk::{ClientConfig, LangfuseClient};

/// Folder names are part of the prompt name; the SDK URL-encodes the `/`.
const PROMPT_NAME: &str = "rust-sdk-examples/label-demo";

type BoxError = Box<dyn std::error::Error>;

fn build_client() -> Result<LangfuseClient, BoxError> {
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

fn text_version(prompt: &str, message: &str) -> CreatePromptRequest {
    CreatePromptRequest::Text {
        name: PROMPT_NAME.into(),
        prompt: prompt.into(),
        config: Some(serde_json::json!({"temperature": 0.2})),
        labels: Some(vec!["staging".into()]),
        tags: Some(vec!["rust-sdk-example".into()]),
        commit_message: Some(message.into()),
    }
}

fn describe(prompt: &Prompt) -> String {
    match prompt {
        Prompt::Text { base, prompt } => {
            format!("v{} {:?}: {prompt}", base.version, base.labels)
        }
        Prompt::Chat { base, prompt } => {
            format!(
                "v{} {:?}: {} messages",
                base.version,
                base.labels,
                prompt.len()
            )
        }
    }
}

fn version_of(prompt: &Prompt) -> i64 {
    match prompt {
        Prompt::Text { base, .. } | Prompt::Chat { base, .. } => base.version,
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // Each create adds a version; labels are unique, so `staging` moves to v2.
    let v1 = client
        .prompts
        .create(&text_version("Summarize: {{text}}", "First draft"))
        .await?;
    println!("Created {}", describe(&v1));

    let outcome = async {
        let v2 = client
            .prompts
            .create(&text_version(
                "Summarize in three bullet points: {{text}}",
                "Shorter, structured output",
            ))
            .await?;
        println!("Created {}", describe(&v2));

        // Promote v1 to production.
        let promoted = client
            .prompt_versions
            .update(
                PROMPT_NAME,
                version_of(&v1),
                &UpdatePromptVersionRequest {
                    new_labels: vec!["production".into()],
                },
            )
            .await?;
        println!("\nPromoted {}", describe(&promoted));

        let production = client
            .prompts
            .get_with_params(
                PROMPT_NAME,
                &GetPromptParams {
                    label: Some("production".into()),
                    ..Default::default()
                },
            )
            .await?;
        println!("label=production -> {}", describe(&production));

        let by_version = client
            .prompts
            .get_with_params(
                PROMPT_NAME,
                &GetPromptParams {
                    version: Some(version_of(&v2)),
                    resolve: Some(false),
                    ..Default::default()
                },
            )
            .await?;
        println!("version={} -> {}", version_of(&v2), describe(&by_version));

        // Structured filter: every prompt in the folder.
        let filter = serde_json::json!([{
            "type": "string",
            "column": "name",
            "operator": "starts with",
            "value": "rust-sdk-examples/"
        }]);
        let listed = client
            .prompts
            .list(&ListPromptsParams {
                filter: Some(filter.to_string()),
                ..Default::default()
            })
            .await?;
        println!("\nPrompts in rust-sdk-examples/:");
        for meta in &listed.data {
            println!(
                "  {} versions {:?} labels {:?} (updated {})",
                meta.name,
                meta.versions,
                meta.labels,
                meta.last_updated_at.format("%Y-%m-%d %H:%M")
            );
        }

        // Delete only v2, then check what is left.
        client
            .prompts
            .delete_with_params(
                PROMPT_NAME,
                &DeletePromptParams {
                    version: Some(version_of(&v2)),
                    label: None,
                },
            )
            .await?;
        println!("\nDeleted version {}", version_of(&v2));
        Ok::<(), BoxError>(())
    }
    .await;

    // Clean up every remaining version.
    client.prompts.delete(PROMPT_NAME).await?;
    println!("Deleted prompt {PROMPT_NAME}");

    outcome
}
