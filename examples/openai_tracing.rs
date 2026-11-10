//! Trace OpenAI calls made with async-openai: a chat completion, a streamed
//! one and an embedding, recorded as generations under one Langfuse
//! observation with model, usage, input and output. Prints the trace URL.
//!
//! Needs `OPENAI_API_KEY`; `OPENAI_BASE_URL` points async-openai at another
//! OpenAI-compatible server and `OPENAI_MODEL` picks the chat model (default
//! `gpt-4o-mini`).
//!
//! Writes data: one trace, kept so you can open it. Delete it in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... OPENAI_API_KEY=sk-... \
//!   cargo run --example openai_tracing --features openai
//! ```

use async_openai::types::chat::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::types::embeddings::CreateEmbeddingRequestArgs;
use futures::StreamExt;
use langfuse_rust_sdk::openai::OpenAI;
use langfuse_rust_sdk::otel::{
    ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let langfuse = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = langfuse.tracer(TracingOptions::from_env()?)?;
    // OPENAI_API_KEY, OPENAI_BASE_URL
    let openai = OpenAI::new(async_openai::Client::new(), &tracer);
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());

    let request = tracer.start_observation("travel-assistant", ObservationType::Agent);
    let user = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_tags(["rust-sdk", "example", "openai"]);
    let summary = request
        .in_scope(propagate_attributes(user, async {
            let question = "Name one museum in Lisbon.";
            let chat = CreateChatCompletionRequestArgs::default()
                .model(&model)
                .temperature(0.2_f32)
                .max_completion_tokens(60_u32)
                .messages([
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content("Answer in one short sentence.")
                        .build()?
                        .into(),
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(question)
                        .build()?
                        .into(),
                ])
                .build()?;
            let answer = openai
                .chat()
                .with_name("answer")
                .with_metadata("question_kind", "travel")
                .create(chat.clone())
                .await?;
            let answer = answer.choices[0]
                .message
                .content
                .clone()
                .unwrap_or_default();

            // Streams are recorded when they end (or are dropped), with the
            // time of the first chunk.
            let mut stream = openai
                .chat()
                .with_name("answer-streamed")
                .create_stream(chat)
                .await?;
            let mut streamed = String::new();
            while let Some(chunk) = stream.next().await {
                for choice in chunk?.choices {
                    streamed.push_str(choice.delta.content.as_deref().unwrap_or_default());
                }
            }

            let embedding = CreateEmbeddingRequestArgs::default()
                .model("text-embedding-3-small")
                .input([answer.as_str(), streamed.as_str()])
                .build()?;
            openai.embeddings().create(embedding).await?;
            Ok::<_, Box<dyn std::error::Error>>(format!("{answer} / {streamed}"))
        }))
        .await?;
    request.set_output(&summary);
    request.end();

    tracer.shutdown().await;
    println!("Answers: {summary}");
    println!("URL: {}", langfuse.trace_url(request.trace_id()).await?);
    Ok(())
}
