//! Release management with prompt labels: create two versions of a prompt in
//! a folder, promote version 1 to `production`, fetch by label and by
//! version, find the prompt with a structured filter, delete one version, and
//! finally the whole prompt.
//!
//! **Writes data**, then cleans up: the prompt `rust-sdk-examples/label-demo`
//! is deleted at the end (all of its versions), also when a step in between
//! fails.
//!
//! Labels are how a version is deployed: applications fetch a prompt by label
//! (`production` when none is given), so moving a label to another version
//! changes what they get without a code change. A label points to one version
//! of a prompt; `latest` is reserved and always follows the newest version.
//!
//! **Docs:** <https://langfuse.com/docs/prompt-management/features/prompt-version-control>,
//! <https://langfuse.com/docs/prompt-management/features/folders>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example prompt_labels
//! ```

use langfuse_rust_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_rust_sdk::api::prompts::{DeletePromptParams, GetPromptParams, ListPromptsParams};
use langfuse_rust_sdk::models::prompts::{CreatePromptRequest, Prompt};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

/// A `/` in the name puts the prompt into a folder in the UI. The folder is
/// part of the name: pass the full name to every call, the SDK URL-encodes it.
const PROMPT_NAME: &str = "rust-sdk-examples/label-demo";

type BoxError = Box<dyn std::error::Error>;

fn build_client() -> Result<LangfuseClient, BoxError> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
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

    // Each create adds a version. Both ask for the `staging` label, and a
    // label points to one version only, so it moves from v1 to v2 (as does
    // `latest`).
    let v1 = client
        .prompts
        .create(&text_version("Summarize: {{text}}", "First draft"))
        .await?;
    println!("Created {}", describe(&v1));

    // Run the rest inside a block, so that the prompt is deleted below even
    // when one of these steps fails.
    let outcome = async {
        let v2 = client
            .prompts
            .create(&text_version(
                "Summarize in three bullet points: {{text}}",
                "Shorter, structured output",
            ))
            .await?;
        println!("Created {}", describe(&v2));

        // Promote v1 to production: set the labels of an existing version.
        // This also invalidates the prompt in the client-side cache.
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

        // By label: what an application does (`client.prompts.get(name)`
        // asks for `production` too).
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

        // By version number: pins the exact text. `label` and `version`
        // cannot be combined. `resolve: false` returns the prompt as stored,
        // without inlining the prompts it references (prompt composability);
        // it is meant for debugging and one-off jobs, not for runtime fetches.
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

        // Structured filter, a JSON array of conditions passed as a string:
        // every prompt in the folder.
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

        // Delete only v2. `label` instead of `version` would delete every
        // version carrying that label.
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

    // Clean up: without `version` or `label`, every remaining version of the
    // prompt is deleted.
    client.prompts.delete(PROMPT_NAME).await?;
    println!("Deleted prompt {PROMPT_NAME}");

    outcome
}
