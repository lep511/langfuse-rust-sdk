//! Prompt management: create a text prompt and a chat prompt, fetch them by
//! label, through the client-side cache and with a fallback, and compile
//! their `{{variables}}` and message placeholders into what a model call
//! needs. Finally list the project's prompts.
//!
//! **Writes data:** every run adds a version to the prompts
//! `rust-sdk-greeting` and `rust-sdk-assistant`, which are kept. Delete them
//! in the UI or with `client.prompts.delete(name)` (all versions).
//!
//! See `prompt_labels` for promoting versions with labels.
//!
//! **Docs:** <https://langfuse.com/docs/prompt-management/get-started>,
//! <https://langfuse.com/docs/prompt-management/features/caching>
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

    // A text prompt is one string with `{{variables}}`. Creating a prompt
    // whose name exists adds a new version, which always gets the `latest`
    // label; `labels` adds more (here `staging`). `config` is free-form JSON
    // versioned with the prompt, typically the model parameters.
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

    // A chat prompt is a list of messages. A placeholder marks the position
    // where a list of messages (e.g. the chat history) is inserted when the
    // prompt is compiled.
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

    // Fetch by name and label, straight from the API. Without a label or a
    // version (`client.prompts.get(name)`) the API returns the version
    // labelled `production`, or 404 when no version has that label.
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

    // What application code should use: `get_cached` keeps prompts in memory
    // (60 s by default, `LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS` or
    // `with_cache_ttl`), so the second call makes no request. An expired entry
    // is still returned at once while a background task refreshes it. The
    // fallback is used only if the prompt can be neither fetched nor found in
    // the cache; `is_fallback()` tells the two apart.
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
    // `variables()` lists the `{{names}}` of the template; `compile`
    // replaces them and leaves variables without a value as they are.
    println!("  Variables: {:?}", greeting.variables());
    let compiled = greeting.compile(&json!({"name": "Ada", "service": "Langfuse"}))?;
    println!("  Compiled:  {}", compiled.as_text().unwrap_or_default());

    // Chat prompt: fill the placeholder with the conversation so far.
    // Variables are replaced in the inserted messages too.
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

    // One entry per prompt name, with its versions and the labels in use
    // (page-based pagination; `name`, `label` and `tag` filter the list).
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
