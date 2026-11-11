//! Use async-openai with the Langfuse AI Gateway (beta): the configuration
//! from `GatewayClient::openai_config` points the client at the gateway's
//! OpenAI route with the gateway key, and adds the trace context of the
//! current observation to every request, so the gateway's generation is a
//! child of the SDK span around the call. The example sends one Responses
//! API request.
//!
//! **Writes data**, when a model is called: one SDK trace (one span) in the
//! project of the API keys and one gateway generation in the gateway's
//! ingestion project. They are kept so you can open them. Delete the trace in
//! the UI or with `client.traces.delete(trace_id)`. The model call is billed
//! by your provider.
//!
//! **Needs:** the `openai` feature, and `LANGFUSE_GATEWAY_KEY` plus
//! `LANGFUSE_GATEWAY_HOST` besides the usual keys. The model is Claude Sonnet
//! 5.5, else Claude Opus 5.5, when the gateway's OpenAI route offers them, or
//! the one `GATEWAY_OPENAI_MODEL` names (e.g.
//! `GATEWAY_OPENAI_MODEL=<connection>:<model>`). The gateway does not
//! translate formats, so the OpenAI route only has models of providers with
//! an OpenAI-compatible API; without one of those Claude models the example
//! prints a note and stops before calling a model (it never picks one
//! Langfuse has no price for, whose cost would count as $0).
//!
//! **Docs:** <https://langfuse.com/integrations/model-providers/openai-gateway>,
//! <https://langfuse.com/docs/gateway/features/tracing>
//!
//! ```sh
//! LANGFUSE_GATEWAY_HOST=https://gateway.cloud.langfuse.com LANGFUSE_GATEWAY_KEY=... \
//!   cargo run --example gateway_openai --features openai
//! ```

use async_openai::types::responses::CreateResponseArgs;
use langfuse_rust_sdk::gateway::{GatewayClient, GatewayHeaders};
use langfuse_rust_sdk::otel::{ObservationType, TracingOptions};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;
    let tracer = client.tracer(TracingOptions::from_env()?)?;
    let gateway = GatewayClient::from_env()?;

    // The model: the one named by the environment, else a Claude 5.5 model
    // of the OpenAI route's list. IDs have the form `<connection>:<model>`.
    let model = match std::env::var("GATEWAY_OPENAI_MODEL") {
        Ok(model) => Some(model),
        Err(_) => {
            let models = gateway.openai_models::<Value>().await?.body;
            let ids: Vec<&str> = models["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str())
                .collect();
            [
                "us.anthropic.claude-sonnet-5-5",
                "claude-sonnet-5-5",
                "us.anthropic.claude-opus-5-5",
                "claude-opus-5-5",
            ]
            .iter()
            .find_map(|needle| ids.iter().find(|id| id.contains(needle)))
            .map(|id| (*id).to_owned())
        }
    };
    let Some(model) = model else {
        println!(
            "the gateway's OpenAI route offers neither Claude Sonnet 5.5 nor Claude Opus 5.5 \
             (they are on the Anthropic route, see the `gateway` example); set \
             GATEWAY_OPENAI_MODEL to call another model"
        );
        tracer.shutdown().await;
        return Ok(());
    };

    // The plain async-openai client: the gateway records the call, so the
    // SDK's `openai::OpenAI` wrapper would record it a second time. Headers
    // set here (a tag) are sent with every request of this client; the trace
    // context is read from the current observation, per request.
    let config = gateway
        .with_headers(GatewayHeaders::default().with_tag("async-openai"))
        .openai_config();
    let openai = async_openai::Client::with_config(config);

    // The request future must run inside the span's scope for the config to
    // see the span: hence `span.in_scope(..)` around the call.
    let span = tracer.start_observation("summarize", ObservationType::Span);
    let request = CreateResponseArgs::default()
        .model(model.as_str())
        .input("Summarize in one sentence what an AI gateway does.")
        .build()?;
    let response = span.in_scope(openai.responses().create(request)).await?;
    span.end();

    println!("{model}: {}", response.output_text().unwrap_or_default());
    println!("SDK trace: {}", client.trace_url(span.trace_id()).await?);
    tracer.shutdown().await;
    Ok(())
}
