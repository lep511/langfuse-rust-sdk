//! Trace Amazon Bedrock Converse calls made with aws-sdk-bedrockruntime: a
//! call and a streamed one, recorded as generations under one Langfuse
//! observation with model, inference parameters, usage, input and output.
//! Prints the question, both answers and the trace URL.
//!
//! Needs AWS credentials and a region with Bedrock access (the usual AWS
//! configuration: environment, profile or instance role). `BEDROCK_MODEL_ID`
//! picks the model or inference profile (default
//! `us.anthropic.claude-sonnet-5-5`).
//!
//! Writes data: one trace, kept so you can open it. Delete it in the UI or with
//! `client.traces.delete(trace_id)`.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... AWS_REGION=us-east-1 \
//!   cargo run -p langfuse-rust-sdk-integration-tests --example bedrock_tracing --features bedrock
//! ```

use aws_sdk_bedrockruntime::types::{
    ContentBlock, ContentBlockDelta, ConversationRole, ConverseStreamOutput,
    InferenceConfiguration, Message, SystemContentBlock,
};
use langfuse_rust_sdk::bedrock::Bedrock;
use langfuse_rust_sdk::otel::{
    ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

const QUESTION: &str = "Name one museum in Lisbon.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let langfuse = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = langfuse.tracer(TracingOptions::from_env()?)?;
    let aws = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let bedrock = Bedrock::new(aws_sdk_bedrockruntime::Client::new(&aws), &tracer);
    let model = std::env::var("BEDROCK_MODEL_ID")
        .unwrap_or_else(|_| "us.anthropic.claude-sonnet-5-5".into());

    println!("question: {QUESTION}");
    let request = tracer.start_observation("travel-assistant", ObservationType::Agent);
    request.set_input(QUESTION);
    let user = PropagatedAttributes::default()
        .with_user_id("user-42")
        .with_tags(["rust-sdk", "example", "bedrock"]);
    let summary = request
        .in_scope(propagate_attributes(user, async {
            let question = Message::builder()
                .role(ConversationRole::User)
                .content(ContentBlock::Text(QUESTION.into()))
                .build()?;
            let system = SystemContentBlock::Text("Answer in one short sentence.".into());
            // Recent Claude models reject `temperature` and `top_p`: set them
            // only for models that accept them.
            let inference = InferenceConfiguration::builder().max_tokens(60).build();

            let response = bedrock
                .converse(|r| {
                    r.model_id(&model)
                        .system(system.clone())
                        .messages(question.clone())
                        .inference_config(inference.clone())
                })
                .with_name("answer")
                .with_metadata("question_kind", "travel")
                .send()
                .await?;
            let answer = match response.output() {
                Some(aws_sdk_bedrockruntime::types::ConverseOutput::Message(message)) => message
                    .content
                    .iter()
                    .filter_map(|block| block.as_text().ok().cloned())
                    .collect::<String>(),
                _ => String::new(),
            };

            // Streams are recorded when they end (or are dropped), with the
            // time of the first token.
            let mut stream = bedrock
                .converse_stream(|r| {
                    r.model_id(&model)
                        .system(system)
                        .messages(question)
                        .inference_config(inference)
                })
                .with_name("answer-streamed")
                .send()
                .await?;
            let mut streamed = String::new();
            while let Some(event) = stream.recv().await? {
                if let ConverseStreamOutput::ContentBlockDelta(delta) = event
                    && let Some(ContentBlockDelta::Text(text)) = delta.delta
                {
                    streamed.push_str(&text);
                }
            }
            Ok::<_, Box<dyn std::error::Error>>((answer, streamed))
        }))
        .await;
    match &summary {
        Ok((answer, streamed)) => {
            request.set_output(answer);
            println!("answer: {answer}");
            println!("streamed answer: {streamed}");
        }
        // The failed generation is recorded with level ERROR; keep the trace.
        Err(error) => {
            request.record_error(error);
        }
    }
    request.end();

    // Export the trace even when a call failed.
    let trace_id = request.trace_id().to_owned();
    tracer.shutdown().await;
    println!("trace: {}", langfuse.trace_url(&trace_id).await?);
    summary.map(|_| ())
}
