//! Traced Amazon Bedrock calls through
//! [`aws-sdk-bedrockruntime`](https://docs.rs/aws-sdk-bedrockruntime)
//! (feature `bedrock`).
//!
//! [`Bedrock`] wraps an `aws_sdk_bedrockruntime::Client` and records every
//! Converse and ConverseStream call as a Langfuse generation: the messages
//! (with the system prompt and the tools), the model ID, the inference
//! parameters, the output message, the token usage with cached tokens and,
//! for streams, the time of the first token. The generation is a child of the
//! current observation (it is not made current), like the
//! [OpenAI integration](crate::openai). Every model that Bedrock serves
//! through Converse is covered: Anthropic Claude, Amazon Nova, Meta Llama,
//! Mistral, Cohere, DeepSeek, ...
//!
//! ```rust,no_run
//! use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message};
//! use langfuse_rust_sdk::bedrock::Bedrock;
//! use langfuse_rust_sdk::otel::TracingOptions;
//! use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
//!
//! # async fn example(client: aws_sdk_bedrockruntime::Client) -> Result<(), Box<dyn std::error::Error>> {
//! let langfuse = LangfuseClient::new(ClientConfig::from_env()?)?;
//! let tracer = langfuse.tracer(TracingOptions::from_env()?)?;
//! // `client` is built from the AWS configuration (e.g. `aws_config::load_defaults`).
//! let bedrock = Bedrock::new(client, &tracer);
//!
//! let message = Message::builder()
//!     .role(ConversationRole::User)
//!     .content(ContentBlock::Text("Say hello".into()))
//!     .build()?;
//! let response = bedrock
//!     .converse(|request| {
//!         request
//!             .model_id("us.anthropic.claude-opus-5-5")
//!             .messages(message)
//!     })
//!     .with_name("greeting")
//!     .send()
//!     .await?;
//! println!("{:?}", response.output());
//! tracer.shutdown().await;
//! # Ok(())
//! # }
//! ```
//!
//! # What is recorded
//!
//! - **Input**: the messages as chat messages (`role`, `content`), after one
//!   `system` message per system text block. A single text block becomes the
//!   content string; other content is a list of parts: `text`, `image_url`
//!   (a data URI, uploaded as media by the tracer), `document`, `audio`,
//!   `video`, `tool_use`, `tool_result`, `reasoning` and `cache_point`. When
//!   tools are offered the input is `{tools, tool_choice, messages}`, with
//!   the tools in the Bedrock request shape (`toolSpec`).
//! - **Model**: the model ID (or inference profile or ARN) of the request.
//! - **Model parameters**: `max_tokens`, `temperature`, `top_p` and `stop`
//!   from `inferenceConfig`, and the top-level keys of
//!   `additionalModelRequestFields` (e.g. `top_k`, `thinking`).
//! - **Output**: the assistant message, in the same shape as the input
//!   messages. Streams accumulate the text, reasoning and tool-use input of
//!   each content block.
//! - **Usage**: `input`, `output` and `total`, plus `input_cache_read` and
//!   `input_cache_creation` for prompt caching: the keys the Python SDK
//!   records for Bedrock through LangChain, which Langfuse prices. Costs are
//!   computed by Langfuse from the model.
//! - **Metadata**: the `stop_reason` and the `latency_ms` of the response.
//! - **Errors**: level `ERROR` with the full error message, and zero cost;
//!   the error is returned unchanged. An error in the middle of a stream
//!   marks the generation the same way.
//!
//! Per call, [`Converse::with_name`], [`Converse::with_metadata`],
//! [`Converse::with_prompt`] (fallback prompts are not linked) and
//! [`Converse::with_trace_context`] (and the same methods of
//! [`ConverseStream`]) set the observation name, metadata, prompt and trace.
//! Set user, session or tags with
//! [`propagate_attributes`](crate::otel::propagate_attributes).
//!
//! Other Bedrock operations (`InvokeModel`, ...) are not traced: trace them
//! by hand and record the usage with
//! [`Observation::set_provider_usage`], which understands the Bedrock and
//! Anthropic usage shapes.

// The calls return the SDK's own (large, unboxed) error types unchanged.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fmt;

use aws_sdk_bedrockruntime::Client;
use aws_sdk_bedrockruntime::error::{DisplayErrorContext, SdkError};
use aws_sdk_bedrockruntime::operation::converse::builders::ConverseFluentBuilder;
use aws_sdk_bedrockruntime::operation::converse::{ConverseError, ConverseOutput};
use aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamError;
use aws_sdk_bedrockruntime::operation::converse_stream::builders::ConverseStreamFluentBuilder;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::primitives::event_stream::EventReceiver;
use aws_sdk_bedrockruntime::types::error::ConverseStreamOutputError;
use aws_sdk_bedrockruntime::types::{
    AudioSource, ContentBlock, ContentBlockDelta, ContentBlockStart, ConverseOutput as OutputUnion,
    ConverseStreamOutput as StreamEvent, DocumentBlock, DocumentContentBlock, DocumentSource,
    ImageBlock, ImageSource, InferenceConfiguration, Message, ReasoningContentBlock,
    ReasoningContentBlockDelta, SystemContentBlock, TokenUsage, Tool, ToolChoice,
    ToolConfiguration, ToolInputSchema, ToolResultContentBlock, VideoSource,
};
use aws_smithy_types::event_stream::RawMessage;
use aws_smithy_types::{Document, Number};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use futures_util::stream::{self, Stream};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::models::prompts::Prompt;
use crate::otel::{LangfuseTracer, Observation, ObservationType, TraceContext};

/// Default name of the generations.
const DEFAULT_NAME: &str = "Bedrock-generation";

/// An `aws_sdk_bedrockruntime::Client` whose Converse calls are recorded as
/// Langfuse generations; see the [module docs](self).
///
/// Clones share the client and the tracer. [`client`](Self::client) gives the
/// wrapped client for calls that should not be traced.
#[derive(Clone)]
pub struct Bedrock {
    client: Client,
    tracer: LangfuseTracer,
}

impl fmt::Debug for Bedrock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bedrock").finish_non_exhaustive()
    }
}

impl Bedrock {
    /// Trace the Converse calls of `client` with `tracer`.
    pub fn new(client: Client, tracer: &LangfuseTracer) -> Self {
        Self {
            client,
            tracer: tracer.clone(),
        }
    }

    /// The wrapped client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// A Converse request (`POST /model/{modelId}/converse`), built by
    /// `build` from `client.converse()`; [`send`](Converse::send) it.
    pub fn converse(
        &self,
        build: impl FnOnce(ConverseFluentBuilder) -> ConverseFluentBuilder,
    ) -> Converse {
        Converse {
            request: build(self.client.converse()),
            tracer: self.tracer.clone(),
            options: CallOptions::default(),
        }
    }

    /// A ConverseStream request (`POST /model/{modelId}/converse-stream`),
    /// built by `build` from `client.converse_stream()`;
    /// [`send`](ConverseStream::send) it.
    pub fn converse_stream(
        &self,
        build: impl FnOnce(ConverseStreamFluentBuilder) -> ConverseStreamFluentBuilder,
    ) -> ConverseStream {
        ConverseStream {
            request: build(self.client.converse_stream()),
            tracer: self.tracer.clone(),
            options: CallOptions::default(),
        }
    }
}

/// Observation settings of one call.
#[derive(Debug, Clone, Default)]
struct CallOptions {
    name: Option<String>,
    metadata: Map<String, Value>,
    prompt: Option<Prompt>,
    trace: Option<TraceContext>,
}

/// The per-call `with_*` methods shared by both requests.
macro_rules! call_options {
    () => {
        /// Name of the generation (default `Bedrock-generation`).
        pub fn with_name(mut self, name: impl Into<String>) -> Self {
            self.options.name = Some(name.into());
            self
        }

        /// Add `metadata[key] = value` to the generation.
        pub fn with_metadata<T: Serialize + ?Sized>(
            mut self,
            key: impl Into<String>,
            value: &T,
        ) -> Self {
            match serde_json::to_value(value) {
                Ok(value) => {
                    self.options.metadata.insert(key.into(), value);
                }
                Err(error) => tracing::warn!(%error, "metadata is not serializable; ignored"),
            }
            self
        }

        /// Link the Langfuse prompt the request was built from (ignored for
        /// fallback prompts).
        pub fn with_prompt(mut self, prompt: &Prompt) -> Self {
            self.options.prompt = Some(prompt.clone());
            self
        }

        /// Record the call in an existing trace, under its parent observation
        /// if set, instead of under the current observation.
        pub fn with_trace_context(mut self, trace: TraceContext) -> Self {
            self.options.trace = Some(trace);
            self
        }
    };
}

/// A traced Converse request; see [`Bedrock::converse`].
pub struct Converse {
    request: ConverseFluentBuilder,
    tracer: LangfuseTracer,
    options: CallOptions,
}

impl fmt::Debug for Converse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Converse")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Converse {
    call_options!();

    /// Send the request (`send()` of the SDK), recorded as a generation.
    pub async fn send(self) -> Result<ConverseOutput, SdkError<ConverseError>> {
        let input = self.request.as_input();
        let request = Request {
            model_id: input.get_model_id().as_deref(),
            messages: input.get_messages().as_deref().unwrap_or_default(),
            system: input.get_system().as_deref().unwrap_or_default(),
            inference: input.get_inference_config().as_ref(),
            tools: input.get_tool_config().as_ref(),
            additional_fields: input.get_additional_model_request_fields().as_ref(),
        };
        let call = Call::start(&self.tracer, &request, self.options);
        let result = self.request.send().await;
        match &result {
            Ok(response) => {
                let output = match response.output() {
                    Some(OutputUnion::Message(message)) => message_json(message),
                    _ => Value::Null,
                };
                call.record(
                    output,
                    response.usage(),
                    Some(response.stop_reason().as_str()),
                    response.metrics().map(|m| m.latency_ms),
                );
                call.observation.end();
            }
            Err(error) => call.fail(error),
        }
        result
    }
}

/// A traced ConverseStream request; see [`Bedrock::converse_stream`].
pub struct ConverseStream {
    request: ConverseStreamFluentBuilder,
    tracer: LangfuseTracer,
    options: CallOptions,
}

impl fmt::Debug for ConverseStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConverseStream")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl ConverseStream {
    call_options!();

    /// Send the request (`send()` of the SDK). The generation ends with the
    /// event stream (or when the [`TracedConverseStream`] is dropped).
    pub async fn send(self) -> Result<TracedConverseStream, SdkError<ConverseStreamError>> {
        let input = self.request.as_input();
        let request = Request {
            model_id: input.get_model_id().as_deref(),
            messages: input.get_messages().as_deref().unwrap_or_default(),
            system: input.get_system().as_deref().unwrap_or_default(),
            inference: input.get_inference_config().as_ref(),
            tools: input.get_tool_config().as_ref(),
            additional_fields: input.get_additional_model_request_fields().as_ref(),
        };
        let call = Call::start(&self.tracer, &request, self.options);
        match self.request.send().await {
            Ok(response) => Ok(TracedConverseStream {
                receiver: response.stream,
                call: Some(call),
                state: StreamState::default(),
            }),
            Err(error) => {
                call.fail(&error);
                Err(error)
            }
        }
    }
}

/// The event stream of a traced ConverseStream call: the events of the
/// SDK's `EventReceiver`, recorded on the generation, which ends when the
/// stream ends, fails or is dropped.
pub struct TracedConverseStream {
    receiver: EventReceiver<StreamEvent, ConverseStreamOutputError>,
    call: Option<Call>,
    state: StreamState,
}

impl fmt::Debug for TracedConverseStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TracedConverseStream")
            .finish_non_exhaustive()
    }
}

/// Error of [`TracedConverseStream::recv`] (the SDK's event-stream error).
pub type ConverseStreamRecvError = SdkError<ConverseStreamOutputError, RawMessage>;

impl TracedConverseStream {
    /// The next event, `None` at the end of the stream (like the SDK's
    /// `EventReceiver::recv`).
    pub async fn recv(&mut self) -> Result<Option<StreamEvent>, ConverseStreamRecvError> {
        match self.receiver.recv().await {
            Ok(Some(event)) => {
                if self
                    .call
                    .as_ref()
                    .is_some_and(|call| call.observation.is_recording())
                {
                    self.state.apply(&event);
                }
                Ok(Some(event))
            }
            Ok(None) => {
                self.finish();
                Ok(None)
            }
            Err(error) => {
                if let Some(call) = self.call.take() {
                    call.fail(&error);
                }
                Err(error)
            }
        }
    }

    /// The events as a [`Stream`].
    pub fn into_stream(self) -> impl Stream<Item = Result<StreamEvent, ConverseStreamRecvError>> {
        stream::unfold(Some(self), |state| async move {
            let mut stream = state?;
            match stream.recv().await {
                Ok(Some(event)) => Some((Ok(event), Some(stream))),
                Ok(None) => None,
                Err(error) => Some((Err(error), None)),
            }
        })
    }

    fn finish(&mut self) {
        let Some(call) = self.call.take() else {
            return;
        };
        if let Some(time) = self.state.first_token {
            call.observation.set_completion_start_time(time);
        }
        call.record(
            self.state.output(),
            self.state.usage.as_ref(),
            self.state.stop_reason.as_deref(),
            self.state.latency_ms,
        );
        call.observation.end();
    }
}

impl Drop for TracedConverseStream {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The parts of a Converse or ConverseStream request that are recorded.
struct Request<'a> {
    model_id: Option<&'a str>,
    messages: &'a [Message],
    system: &'a [SystemContentBlock],
    inference: Option<&'a InferenceConfiguration>,
    tools: Option<&'a ToolConfiguration>,
    additional_fields: Option<&'a Document>,
}

/// The generation of one call.
struct Call {
    observation: Observation,
}

impl Call {
    fn start(tracer: &LangfuseTracer, request: &Request<'_>, options: CallOptions) -> Self {
        let name = options.name.unwrap_or_else(|| DEFAULT_NAME.to_owned());
        let observation = match &options.trace {
            Some(trace) => {
                tracer.start_observation_in_trace(name, ObservationType::Generation, trace)
            }
            None => tracer.start_observation(name, ObservationType::Generation),
        };
        if observation.is_recording() {
            observation.set_input(&request_input(request));
            if let Some(model) = request.model_id.filter(|m| !m.is_empty()) {
                observation.set_model(model);
            }
            let parameters = model_parameters(request.inference, request.additional_fields);
            if !parameters.is_empty() {
                observation.set_model_parameters(&parameters);
            }
            if !options.metadata.is_empty() {
                observation.set_metadata(&options.metadata);
            }
            if let Some(prompt) = &options.prompt {
                observation.set_prompt(prompt);
            }
        }
        Self { observation }
    }

    /// Record the response (the generation is ended by the caller).
    fn record(
        &self,
        output: Value,
        usage: Option<&TokenUsage>,
        stop_reason: Option<&str>,
        latency_ms: Option<i64>,
    ) {
        let observation = &self.observation;
        if !observation.is_recording() {
            return;
        }
        observation.set_output(&output);
        if let Some(usage) = usage {
            observation.set_usage_details(&usage_details(usage));
        }
        let mut metadata = Map::new();
        if let Some(reason) = stop_reason {
            metadata.insert("stop_reason".into(), reason.into());
        }
        if let Some(latency) = latency_ms {
            metadata.insert("latency_ms".into(), latency.into());
        }
        if !metadata.is_empty() {
            observation.set_metadata(&metadata);
        }
    }

    fn fail<E, R>(&self, error: &SdkError<E, R>)
    where
        E: std::error::Error + 'static,
        R: fmt::Debug,
    {
        let message = DisplayErrorContext(error).to_string();
        tracing::warn!(error = %message, "Bedrock call failed");
        self.observation
            .record_error_parts("SdkError", message)
            .set_cost_details(&json!({"input": 0, "output": 0, "total": 0}));
        self.observation.end();
    }
}

/// Token usage in the keys Langfuse prices: `input` holds the uncached input
/// tokens (as Bedrock reports them), the cache reads and writes are apart.
fn usage_details(usage: &TokenUsage) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("input".into(), usage.input_tokens.into());
    details.insert("output".into(), usage.output_tokens.into());
    details.insert("total".into(), usage.total_tokens.into());
    if let Some(read) = usage.cache_read_input_tokens {
        details.insert("input_cache_read".into(), read.into());
    }
    if let Some(write) = usage.cache_write_input_tokens {
        details.insert("input_cache_creation".into(), write.into());
    }
    details
}

fn model_parameters(
    inference: Option<&InferenceConfiguration>,
    additional_fields: Option<&Document>,
) -> Map<String, Value> {
    let mut parameters = Map::new();
    if let Some(config) = inference {
        if let Some(max_tokens) = config.max_tokens {
            parameters.insert("max_tokens".into(), max_tokens.into());
        }
        if let Some(temperature) = config.temperature {
            parameters.insert("temperature".into(), short_float(temperature));
        }
        if let Some(top_p) = config.top_p {
            parameters.insert("top_p".into(), short_float(top_p));
        }
        if let Some(stop) = config.stop_sequences.as_ref().filter(|s| !s.is_empty()) {
            parameters.insert("stop".into(), json!(stop));
        }
    }
    if let Some(Document::Object(fields)) = additional_fields {
        for (key, value) in fields {
            parameters.insert(key.clone(), document_json(value));
        }
    }
    parameters
}

/// An `f32` as its shortest decimal form (`0.7`, not `0.699999988079071`).
fn short_float(value: f32) -> Value {
    value
        .to_string()
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map_or(Value::Null, Value::Number)
}

fn request_input(request: &Request<'_>) -> Value {
    let mut messages: Vec<Value> = request
        .system
        .iter()
        .filter_map(|block| match block {
            SystemContentBlock::Text(text) => Some(json!({"role": "system", "content": text})),
            _ => None,
        })
        .collect();
    messages.extend(request.messages.iter().map(message_json));
    let Some(tools) = request.tools else {
        return Value::Array(messages);
    };
    let mut input = Map::new();
    input.insert(
        "tools".into(),
        Value::Array(tools.tools.iter().filter_map(tool_json).collect()),
    );
    if let Some(choice) = &tools.tool_choice {
        input.insert("tool_choice".into(), tool_choice_json(choice));
    }
    input.insert("messages".into(), Value::Array(messages));
    Value::Object(input)
}

fn tool_json(tool: &Tool) -> Option<Value> {
    match tool {
        Tool::ToolSpec(spec) => {
            let mut out = Map::new();
            out.insert("name".into(), spec.name.clone().into());
            if let Some(description) = &spec.description {
                out.insert("description".into(), description.clone().into());
            }
            if let Some(ToolInputSchema::Json(schema)) = &spec.input_schema {
                out.insert("inputSchema".into(), json!({"json": document_json(schema)}));
            }
            Some(json!({"toolSpec": out}))
        }
        Tool::CachePoint(_) => Some(json!({"cachePoint": {"type": "default"}})),
        _ => None,
    }
}

fn tool_choice_json(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto(_) => json!({"auto": {}}),
        ToolChoice::Any(_) => json!({"any": {}}),
        ToolChoice::Tool(tool) => json!({"tool": {"name": tool.name}}),
        _ => Value::Null,
    }
}

fn message_json(message: &Message) -> Value {
    json!({
        "role": message.role.as_str(),
        "content": content_json(&message.content),
    })
}

/// A single text block as a string, any other content as a list of parts.
fn content_json(blocks: &[ContentBlock]) -> Value {
    if let [ContentBlock::Text(text)] = blocks {
        return Value::String(text.clone());
    }
    Value::Array(blocks.iter().map(block_json).collect())
}

fn block_json(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text(text) => json!({"type": "text", "text": text}),
        ContentBlock::Image(image) => image_json(image),
        ContentBlock::Document(document) => document_block_json(document),
        ContentBlock::Audio(audio) => {
            let format = audio.format.as_str();
            let data = match &audio.source {
                Some(AudioSource::Bytes(bytes)) => data_uri(&audio_mime(format), bytes).into(),
                _ => Value::Null,
            };
            json!({"type": "audio", "format": format, "data": data})
        }
        ContentBlock::Video(video) => {
            let format = video.format.as_str();
            let source = match &video.source {
                Some(VideoSource::Bytes(bytes)) => data_uri(&video_mime(format), bytes).into(),
                Some(VideoSource::S3Location(location)) => location.uri.clone().into(),
                _ => Value::Null,
            };
            json!({"type": "video", "format": format, "source": source})
        }
        ContentBlock::ToolUse(tool_use) => json!({
            "type": "tool_use",
            "id": tool_use.tool_use_id,
            "name": tool_use.name,
            "input": document_json(&tool_use.input),
        }),
        ContentBlock::ToolResult(result) => {
            let content: Vec<Value> = result
                .content
                .iter()
                .map(|block| match block {
                    ToolResultContentBlock::Text(text) => json!({"type": "text", "text": text}),
                    ToolResultContentBlock::Json(document) => {
                        json!({"type": "json", "json": document_json(document)})
                    }
                    ToolResultContentBlock::Image(image) => image_json(image),
                    ToolResultContentBlock::Document(document) => document_block_json(document),
                    _ => json!({"type": "unknown"}),
                })
                .collect();
            let mut out = json!({
                "type": "tool_result",
                "tool_use_id": result.tool_use_id,
                "content": content,
            });
            if let Some(status) = &result.status {
                out["status"] = status.as_str().into();
            }
            out
        }
        ContentBlock::ReasoningContent(reasoning) => match reasoning {
            ReasoningContentBlock::ReasoningText(text) => {
                json!({"type": "reasoning", "text": text.text})
            }
            ReasoningContentBlock::RedactedContent(_) => {
                json!({"type": "reasoning", "redacted": true})
            }
            _ => json!({"type": "reasoning"}),
        },
        ContentBlock::CachePoint(_) => json!({"type": "cache_point"}),
        ContentBlock::GuardContent(_) => json!({"type": "guard_content"}),
        ContentBlock::CitationsContent(_) => json!({"type": "citations"}),
        ContentBlock::SearchResult(_) => json!({"type": "search_result"}),
        _ => json!({"type": "unknown"}),
    }
}

fn image_json(image: &ImageBlock) -> Value {
    let format = image.format.as_str();
    let url = match &image.source {
        Some(ImageSource::Bytes(bytes)) => data_uri(&format!("image/{format}"), bytes),
        Some(ImageSource::S3Location(location)) => location.uri.clone(),
        _ => String::new(),
    };
    json!({"type": "image_url", "image_url": {"url": url}})
}

fn document_block_json(document: &DocumentBlock) -> Value {
    let format = document.format.as_str();
    let source = match &document.source {
        Some(DocumentSource::Bytes(bytes)) => data_uri(document_mime(format), bytes).into(),
        Some(DocumentSource::Text(text)) => text.clone().into(),
        Some(DocumentSource::Content(blocks)) => Value::Array(
            blocks
                .iter()
                .filter_map(|block| match block {
                    DocumentContentBlock::Text(text) => Some(json!({"type": "text", "text": text})),
                    _ => None,
                })
                .collect(),
        ),
        _ => Value::Null,
    };
    json!({
        "type": "document",
        "name": document.name,
        "format": format,
        "source": source,
    })
}

fn data_uri(mime: &str, bytes: &Blob) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes.as_ref()))
}

fn document_mime(format: &str) -> &'static str {
    match format {
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "html" => "text/html",
        "md" => "text/markdown",
        _ => "text/plain",
    }
}

fn audio_mime(format: &str) -> String {
    match format {
        "mp3" | "mpga" => "audio/mpeg".into(),
        "m4a" => "audio/mp4".into(),
        "x-aac" => "audio/aac".into(),
        other => format!("audio/{other}"),
    }
}

fn video_mime(format: &str) -> String {
    match format {
        "mov" => "video/quicktime".into(),
        "mkv" => "video/x-matroska".into(),
        "mpg" => "video/mpeg".into(),
        "three_gp" => "video/3gpp".into(),
        "flv" => "video/x-flv".into(),
        "wmv" => "video/x-ms-wmv".into(),
        other => format!("video/{other}"),
    }
}

/// A Smithy `Document` (tool inputs, schemas, model fields) as JSON.
fn document_json(document: &Document) -> Value {
    match document {
        Document::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), document_json(v)))
                .collect(),
        ),
        Document::Array(items) => Value::Array(items.iter().map(document_json).collect()),
        Document::Number(Number::PosInt(n)) => (*n).into(),
        Document::Number(Number::NegInt(n)) => (*n).into(),
        Document::Number(Number::Float(f)) => {
            serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number)
        }
        Document::String(s) => s.clone().into(),
        Document::Bool(b) => (*b).into(),
        Document::Null => Value::Null,
    }
}

/// What a ConverseStream call has produced so far.
#[derive(Debug, Default)]
struct StreamState {
    role: Option<String>,
    /// Content blocks by index.
    blocks: BTreeMap<i32, StreamBlock>,
    stop_reason: Option<String>,
    usage: Option<TokenUsage>,
    latency_ms: Option<i64>,
    first_token: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug)]
enum StreamBlock {
    Text(String),
    Reasoning(String),
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
}

impl StreamState {
    fn apply(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart(start) => self.role = Some(start.role.as_str().to_owned()),
            StreamEvent::ContentBlockStart(start) => {
                if let Some(ContentBlockStart::ToolUse(tool_use)) = &start.start {
                    self.blocks.insert(
                        start.content_block_index,
                        StreamBlock::ToolUse {
                            id: tool_use.tool_use_id.clone(),
                            name: tool_use.name.clone(),
                            input: String::new(),
                        },
                    );
                }
            }
            StreamEvent::ContentBlockDelta(delta) => {
                self.first_token.get_or_insert_with(Utc::now);
                let index = delta.content_block_index;
                match &delta.delta {
                    Some(ContentBlockDelta::Text(text)) => {
                        match self
                            .blocks
                            .entry(index)
                            .or_insert_with(|| StreamBlock::Text(String::new()))
                        {
                            StreamBlock::Text(out) => out.push_str(text),
                            other => tracing::debug!(?other, "text delta for another block"),
                        }
                    }
                    Some(ContentBlockDelta::ReasoningContent(
                        ReasoningContentBlockDelta::Text(text),
                    )) => {
                        if let StreamBlock::Reasoning(out) = self
                            .blocks
                            .entry(index)
                            .or_insert_with(|| StreamBlock::Reasoning(String::new()))
                        {
                            out.push_str(text);
                        }
                    }
                    Some(ContentBlockDelta::ToolUse(tool_use)) => {
                        if let Some(StreamBlock::ToolUse { input, .. }) =
                            self.blocks.get_mut(&index)
                        {
                            input.push_str(&tool_use.input);
                        }
                    }
                    _ => {}
                }
            }
            StreamEvent::MessageStop(stop) => {
                self.stop_reason = Some(stop.stop_reason.as_str().to_owned());
            }
            StreamEvent::Metadata(metadata) => {
                if let Some(usage) = &metadata.usage {
                    self.usage = Some(usage.clone());
                }
                if let Some(metrics) = &metadata.metrics {
                    self.latency_ms = Some(metrics.latency_ms);
                }
            }
            _ => {}
        }
    }

    /// The assistant message accumulated from the stream.
    fn output(&self) -> Value {
        let parts: Vec<Value> = self
            .blocks
            .values()
            .map(|block| match block {
                StreamBlock::Text(text) => json!({"type": "text", "text": text}),
                StreamBlock::Reasoning(text) => json!({"type": "reasoning", "text": text}),
                StreamBlock::ToolUse { id, name, input } => json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    // Streamed as JSON text; kept as text when incomplete.
                    "input": serde_json::from_str::<Value>(input)
                        .unwrap_or_else(|_| Value::String(input.clone())),
                }),
            })
            .collect();
        let content = match parts.as_slice() {
            [part] if part["type"] == "text" => part["text"].clone(),
            _ => Value::Array(parts),
        };
        json!({
            "role": self.role.as_deref().unwrap_or("assistant"),
            "content": content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_bedrockruntime::types::{ImageFormat, ToolUseBlock};

    #[test]
    fn documents_become_json() {
        let document = Document::Object(
            [
                ("n".to_owned(), Document::Number(Number::PosInt(3))),
                ("neg".to_owned(), Document::Number(Number::NegInt(-2))),
                ("f".to_owned(), Document::Number(Number::Float(0.5))),
                (
                    "list".to_owned(),
                    Document::Array(vec![Document::Bool(true), Document::Null]),
                ),
                ("s".to_owned(), Document::String("x".into())),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            document_json(&document),
            json!({"n": 3, "neg": -2, "f": 0.5, "list": [true, null], "s": "x"})
        );
    }

    #[test]
    fn content_blocks_become_chat_parts() {
        let text = vec![ContentBlock::Text("hi".into())];
        assert_eq!(content_json(&text), json!("hi"));

        let image = ImageBlock::builder()
            .format(ImageFormat::Png)
            .source(ImageSource::Bytes(Blob::new(b"png".to_vec())))
            .build()
            .unwrap();
        let tool_use = ToolUseBlock::builder()
            .tool_use_id("t1")
            .name("weather")
            .input(Document::Object(
                [("city".to_owned(), Document::String("Paris".into()))]
                    .into_iter()
                    .collect(),
            ))
            .build()
            .unwrap();
        let blocks = vec![
            ContentBlock::Text("look".into()),
            ContentBlock::Image(image),
            ContentBlock::ToolUse(tool_use),
        ];
        assert_eq!(
            content_json(&blocks),
            json!([
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,cG5n"}},
                {"type": "tool_use", "id": "t1", "name": "weather", "input": {"city": "Paris"}},
            ])
        );
    }

    #[test]
    fn usage_keeps_cache_reads_and_writes_apart() {
        let usage = TokenUsage::builder()
            .input_tokens(10)
            .output_tokens(100)
            .total_tokens(3010)
            .cache_read_input_tokens(2900)
            .cache_write_input_tokens(0)
            .build()
            .unwrap();
        assert_eq!(
            Value::Object(usage_details(&usage)),
            json!({
                "input": 10, "output": 100, "total": 3010,
                "input_cache_read": 2900, "input_cache_creation": 0,
            })
        );
    }

    #[test]
    fn floats_keep_their_short_form() {
        assert_eq!(short_float(0.7), json!(0.7));
        assert_eq!(short_float(1.0), json!(1.0));
    }

    #[test]
    fn mime_types() {
        assert_eq!(document_mime("pdf"), "application/pdf");
        assert_eq!(audio_mime("mp3"), "audio/mpeg");
        assert_eq!(audio_mime("wav"), "audio/wav");
        assert_eq!(video_mime("mov"), "video/quicktime");
        assert_eq!(video_mime("mp4"), "video/mp4");
    }
}
