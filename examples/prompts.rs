//! Create and fetch text and chat prompts.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example prompts
//! ```

use langfuse_sdk::api::prompts::{GetPromptParams, ListPromptsParams};
use langfuse_sdk::models::prompts::{ChatPromptMessage, CreatePromptRequest, Prompt};
use langfuse_sdk::{ClientConfig, LangfuseClient};

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

    // Create a text prompt
    let text_prompt = client
        .prompts
        .create(&CreatePromptRequest::Text {
            name: "rust-sdk-greeting".into(),
            prompt: "Hello {{name}}, welcome to {{service}}! How can I help you today?".into(),
            config: Some(serde_json::json!({"temperature": 0.7, "max_tokens": 500})),
            labels: Some(vec!["staging".into()]),
            tags: Some(vec!["greeting".into(), "rust-sdk".into()]),
            commit_message: Some("Initial greeting prompt".into()),
        })
        .await?;
    println!("Created text prompt:");
    print_prompt(&text_prompt);

    // Create a chat prompt
    let chat_prompt = client
        .prompts
        .create(&CreatePromptRequest::Chat {
            name: "rust-sdk-assistant".into(),
            prompt: vec![
                ChatPromptMessage::message(
                    "system",
                    "You are a helpful coding assistant specializing in {{language}}.",
                ),
                ChatPromptMessage::placeholder("chat_history"),
                ChatPromptMessage::message("user", "{{user_question}}"),
            ],
            config: Some(serde_json::json!({"temperature": 0.3})),
            labels: None,
            tags: Some(vec!["assistant".into(), "rust-sdk".into()]),
            commit_message: Some("Initial assistant prompt".into()),
        })
        .await?;
    println!("\nCreated chat prompt:");
    print_prompt(&chat_prompt);

    // Fetch a prompt by name and label
    println!("\nFetching 'rust-sdk-greeting' with label 'latest'...");
    let fetched = client
        .prompts
        .get_with_params(
            "rust-sdk-greeting",
            &GetPromptParams {
                label: Some("latest".into()),
                ..Default::default()
            },
        )
        .await?;
    print_prompt(&fetched);

    // List all prompts
    let list = client
        .prompts
        .list(&ListPromptsParams {
            limit: Some(5),
            ..Default::default()
        })
        .await?;
    println!(
        "\nAll prompts (page 1): {} of {}",
        list.data.len(),
        list.meta.total_items
    );
    for p in &list.data {
        let versions: Vec<String> = p.versions.iter().map(|v| format!("v{v}")).collect();
        println!(
            "  - {} [{}] [{:?}] ({})",
            p.name,
            versions.join(", "),
            p.r#type,
            p.labels.join(", ")
        );
    }

    Ok(())
}

fn print_prompt(prompt: &Prompt) {
    match prompt {
        Prompt::Text { base, prompt } => {
            println!("  [text] {} v{}", base.name, base.version);
            println!("  Labels: {:?}", base.labels);
            println!(
                "  Template: {}",
                prompt.chars().take(80).collect::<String>()
            );
        }
        Prompt::Chat { base, prompt } => {
            println!("  [chat] {} v{}", base.name, base.version);
            println!("  Labels: {:?}", base.labels);
            println!("  Messages: {} turn(s)", prompt.len());
            for msg in prompt {
                match msg {
                    ChatPromptMessage::Message(m) => {
                        let preview: String = m.content.chars().take(60).collect();
                        println!("    {}: {}", m.role, preview);
                    }
                    ChatPromptMessage::Placeholder(p) => {
                        println!("    <placeholder {}>", p.name);
                    }
                }
            }
        }
    }
}
