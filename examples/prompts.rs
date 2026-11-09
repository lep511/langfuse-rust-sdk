//! Create text and chat prompts, fetch them through the client-side cache and
//! compile their variables and placeholders.
//!
//! Writes data: creates (a new version of) the prompts `rust-sdk-greeting` and
//! `rust-sdk-assistant` and keeps them. Delete them in the UI or with
//! `client.prompts.delete(name)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example prompts
//! ```

use std::sync::Arc;

use langfuse_rust_sdk::api::prompts::{
    GetPromptOptions, GetPromptParams, ListPromptsParams, PromptFallback,
};
use langfuse_rust_sdk::models::prompts::{
    ChatMessage, ChatPromptMessage, CreatePromptRequest, Prompt,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
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

    // Cached fetch: the second call is served from memory. The fallback is used
    // only if the prompt can be neither fetched nor found in the cache.
    let options = GetPromptOptions::default()
        .with_label("staging")
        .with_fallback(PromptFallback::Text("Hello {{name}}!".into()));
    let greeting = client
        .prompts
        .get_cached("rust-sdk-greeting", &options)
        .await?;
    let again = client
        .prompts
        .get_cached("rust-sdk-greeting", &options)
        .await?;
    println!(
        "\nCached 'rust-sdk-greeting' v{} (second call from memory: {}, fallback: {})",
        greeting.version(),
        Arc::ptr_eq(&greeting, &again),
        greeting.is_fallback()
    );
    println!("  Variables: {:?}", greeting.variables());
    let compiled = greeting.compile(&json!({"name": "Ada", "service": "Langfuse"}))?;
    println!("  Compiled:  {}", compiled.as_text().unwrap_or_default());

    // Chat prompt: fill the placeholder with the conversation so far.
    let assistant = client
        .prompts
        .get_cached(
            "rust-sdk-assistant",
            &GetPromptOptions::default().with_label("latest"),
        )
        .await?;
    let history = vec![
        ChatMessage {
            role: "user".into(),
            content: "How do I read a file in {{language}}?".into(),
        },
        ChatMessage {
            role: "assistant".into(),
            content: "Use std::fs::read_to_string.".into(),
        },
    ];
    let compiled = assistant.compile_with_placeholders(
        &json!({"language": "Rust", "user_question": "And asynchronously?"}),
        [("chat_history", &history)],
    )?;
    println!("\nCompiled 'rust-sdk-assistant':");
    for message in compiled.as_chat().unwrap_or_default() {
        if let ChatPromptMessage::Message(m) = message {
            println!("    {}: {}", m.role, m.content);
        }
    }

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
