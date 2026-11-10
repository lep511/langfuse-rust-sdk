//! Traced OpenAI calls through [`async-openai`](https://docs.rs/async-openai)
//! (feature `openai`).
//!
//! [`OpenAI`] wraps an `async_openai::Client` and records every chat
//! completion, Responses API call and embedding as a Langfuse generation (an
//! embedding observation for embeddings): input messages, model, model
//! parameters, output, token usage and, for streams, the time of the first
//! chunk. The observation is a child of the current one (it is not made
//! current), like the OpenAI integration of the Python SDK.
//!
//! ```rust,no_run
//! use async_openai::types::chat::{
//!     ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
//! };
//! use langfuse_rust_sdk::openai::OpenAI;
//! use langfuse_rust_sdk::otel::TracingOptions;
//! use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let langfuse = LangfuseClient::new(ClientConfig::from_env()?)?;
//! let tracer = langfuse.tracer(TracingOptions::from_env()?)?;
//! let openai = OpenAI::new(async_openai::Client::new(), &tracer);
//!
//! let request = CreateChatCompletionRequestArgs::default()
//!     .model("gpt-4o-mini")
//!     .messages([ChatCompletionRequestUserMessageArgs::default()
//!         .content("Say hello")
//!         .build()?
//!         .into()])
//!     .build()?;
//! let response = openai.chat().with_name("greeting").create(request).await?;
//! println!("{:?}", response.choices[0].message.content);
//! tracer.shutdown().await;
//! # Ok(())
//! # }
//! ```
//!
//! # What is recorded
//!
//! - **Input**: the messages, or `{tools, functions, function_call, messages}`
//!   when tools are offered. For the Responses API, `instructions` becomes a
//!   system message before `input`, and `tools`, `tool_choice` and
//!   `parallel_tool_calls` are added. Audio inputs become data URIs, which the
//!   tracer uploads as media. Embeddings record their `input`.
//! - **Model parameters**: `temperature` (1), `max_tokens` (`"Infinity"`),
//!   `top_p` (1), `frequency_penalty` (0) and `presence_penalty` (0), with
//!   those defaults when unset; when set, `max_completion_tokens` (replacing
//!   `max_tokens`), `n` (above 1), `seed`, `service_tier` (the one of the
//!   response wins), `reasoning_effort`, `verbosity`, `reasoning_summary` and
//!   `max_output_tokens` (replacing `max_tokens`). Embeddings record
//!   `dimensions` and a non-`float` `encoding_format`.
//! - **Output**: the message (`role`, `content`, `tool_calls`,
//!   `function_call`, `audio`), a list for several choices, the Responses
//!   API output items, or `{dimensions, count}` for embeddings. Streams
//!   accumulate the content and the tool calls by index and record the
//!   `finish_reason` as metadata.
//! - **Usage**: the usage of the response without null values; Langfuse maps
//!   cached and reasoning tokens. Embedding usage is `{"input": n}`. Costs
//!   are computed by Langfuse from the model.
//! - **Errors**: level `ERROR` with the error message, the requested model
//!   and zero cost; the error is returned unchanged. An error in the middle of
//!   a stream marks the generation the same way.
//!
//! Per call, [`Chat::with_name`], [`Chat::with_metadata`],
//! [`Chat::with_prompt`] (fallback prompts are not linked) and
//! [`Chat::with_trace_context`] (and the same methods of [`Embeddings`] and
//! [`Responses`]) set the observation name, metadata, prompt and trace. Set
//! user, session or tags with
//! [`propagate_attributes`](crate::otel::propagate_attributes).
//!
//! The `async-openai` API changes between 0.x versions: this module is built
//! for 0.43 (`chat-completion`, `embedding` and `responses` features).
//!
//! # Langfuse AI Gateway
//!
//! Calls through the [Langfuse AI Gateway](crate::gateway) are recorded by
//! the gateway itself: do not wrap such a client in [`OpenAI`], or every call
//! is recorded twice. Use a plain `async_openai::Client` with
//! [`GatewayClient::openai_config`](crate::gateway::GatewayClient::openai_config),
//! which sends the trace context of the current observation instead.

use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use async_openai::config::{Config, OpenAIConfig};
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionResponseStream, CreateChatCompletionRequest, CreateChatCompletionResponse,
};
use async_openai::types::embeddings::{CreateEmbeddingRequest, CreateEmbeddingResponse};
use async_openai::types::responses::{CreateResponse, Response, ResponseStream};
use chrono::Utc;
use futures_util::stream::Stream;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::models::prompts::Prompt;
use crate::otel::{LangfuseTracer, Observation, ObservationType, TraceContext};

/// An `async_openai::Client` whose calls are recorded as Langfuse
/// generations; see the [module docs](self).
///
/// Clones share the client and the tracer. [`client`](Self::client) gives
/// the wrapped client for calls that should not be traced.
#[derive(Clone)]
pub struct OpenAI<C: Config = OpenAIConfig> {
    client: async_openai::Client<C>,
    tracer: LangfuseTracer,
}

impl<C: Config> std::fmt::Debug for OpenAI<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAI").finish_non_exhaustive()
    }
}

impl<C: Config> OpenAI<C> {
    /// Trace the calls of `client` with `tracer`.
    pub fn new(client: async_openai::Client<C>, tracer: &LangfuseTracer) -> Self {
        Self {
            client,
            tracer: tracer.clone(),
        }
    }

    /// The wrapped client.
    pub fn client(&self) -> &async_openai::Client<C> {
        &self.client
    }

    /// Chat completions (`POST /chat/completions`).
    pub fn chat(&self) -> Chat<'_, C> {
        Chat {
            openai: self,
            options: CallOptions::default(),
        }
    }

    /// Embeddings (`POST /embeddings`).
    pub fn embeddings(&self) -> Embeddings<'_, C> {
        Embeddings {
            openai: self,
            options: CallOptions::default(),
        }
    }

    /// The Responses API (`POST /responses`).
    pub fn responses(&self) -> Responses<'_, C> {
        Responses {
            openai: self,
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

/// The per-call `with_*` methods shared by the three handles.
macro_rules! call_options {
    () => {
        /// Name of the observation (default `OpenAI-generation`, or
        /// `OpenAI-embedding` for embeddings).
        pub fn with_name(mut self, name: impl Into<String>) -> Self {
            self.options.name = Some(name.into());
            self
        }

        /// Add `metadata[key] = value` to the observation.
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

/// Traced chat completions; see [`OpenAI::chat`].
#[derive(Debug)]
pub struct Chat<'a, C: Config> {
    openai: &'a OpenAI<C>,
    options: CallOptions,
}

impl<C: Config> Chat<'_, C> {
    call_options!();

    /// `client.chat().create(request)`, recorded as a generation.
    pub async fn create(
        self,
        request: CreateChatCompletionRequest,
    ) -> Result<CreateChatCompletionResponse, OpenAIError> {
        let call = Call::start(&self.openai.tracer, Api::Chat, &request, self.options);
        let result = self.openai.client.chat().create(request).await;
        call.finish(&result);
        result
    }

    /// `client.chat().create_stream(request)`, recorded as a generation that
    /// ends with the stream (or when it is dropped).
    pub async fn create_stream(
        self,
        request: CreateChatCompletionRequest,
    ) -> Result<ChatCompletionResponseStream, OpenAIError> {
        let call = Call::start(&self.openai.tracer, Api::Chat, &request, self.options);
        match self.openai.client.chat().create_stream(request).await {
            Ok(stream) => Ok(Box::pin(TracedStream::new(stream, call))),
            Err(error) => {
                call.fail(&error);
                Err(error)
            }
        }
    }
}

/// Traced embeddings; see [`OpenAI::embeddings`].
#[derive(Debug)]
pub struct Embeddings<'a, C: Config> {
    openai: &'a OpenAI<C>,
    options: CallOptions,
}

impl<C: Config> Embeddings<'_, C> {
    call_options!();

    /// `client.embeddings().create(request)`, recorded as an embedding observation.
    pub async fn create(
        self,
        request: CreateEmbeddingRequest,
    ) -> Result<CreateEmbeddingResponse, OpenAIError> {
        let call = Call::start(&self.openai.tracer, Api::Embedding, &request, self.options);
        let result = self.openai.client.embeddings().create(request).await;
        call.finish(&result);
        result
    }
}

/// The traced Responses API; see [`OpenAI::responses`].
#[derive(Debug)]
pub struct Responses<'a, C: Config> {
    openai: &'a OpenAI<C>,
    options: CallOptions,
}

impl<C: Config> Responses<'_, C> {
    call_options!();

    /// `client.responses().create(request)`, recorded as a generation.
    pub async fn create(self, request: CreateResponse) -> Result<Response, OpenAIError> {
        let call = Call::start(&self.openai.tracer, Api::Responses, &request, self.options);
        let result = self.openai.client.responses().create(request).await;
        call.finish(&result);
        result
    }

    /// `client.responses().create_stream(request)`, recorded as a generation
    /// that ends with the stream (or when it is dropped).
    pub async fn create_stream(
        self,
        request: CreateResponse,
    ) -> Result<ResponseStream, OpenAIError> {
        let call = Call::start(&self.openai.tracer, Api::Responses, &request, self.options);
        match self.openai.client.responses().create_stream(request).await {
            Ok(stream) => Ok(Box::pin(TracedStream::new(stream, call))),
            Err(error) => {
                call.fail(&error);
                Err(error)
            }
        }
    }
}

/// Which OpenAI endpoint a call goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Api {
    Chat,
    Responses,
    Embedding,
}

/// The observation of one call.
struct Call {
    observation: Observation,
    api: Api,
    model: Option<String>,
    model_parameters: Map<String, Value>,
}

impl Call {
    fn start<R: Serialize>(
        tracer: &LangfuseTracer,
        api: Api,
        request: &R,
        options: CallOptions,
    ) -> Self {
        let (kind, default_name) = match api {
            Api::Embedding => (ObservationType::Embedding, "OpenAI-embedding"),
            Api::Chat | Api::Responses => (ObservationType::Generation, "OpenAI-generation"),
        };
        let name = options.name.unwrap_or_else(|| default_name.to_owned());
        let observation = match &options.trace {
            Some(trace) => tracer.start_observation_in_trace(name, kind, trace),
            None => tracer.start_observation(name, kind),
        };
        let request = to_json(request);
        let model = request["model"]
            .as_str()
            .filter(|m| !m.is_empty())
            .map(str::to_owned);
        let model_parameters = model_parameters(api, &request);
        if observation.is_recording() {
            match api {
                Api::Chat => observation.set_input(&chat_input(&request)),
                Api::Responses => observation.set_input(&responses_input(&request)),
                Api::Embedding => observation.set_input(&request["input"]),
            };
            if let Some(model) = &model {
                observation.set_model(model.clone());
            }
            observation.set_model_parameters(&model_parameters);
            let mut metadata = options.metadata;
            if let Some(format) = request.get("response_format").filter(|v| !v.is_null()) {
                metadata.insert("response_format".into(), format.clone());
            }
            if !metadata.is_empty() {
                observation.set_metadata(&metadata);
            }
            if let Some(prompt) = &options.prompt {
                observation.set_prompt(prompt);
            }
        }
        Self {
            observation,
            api,
            model,
            model_parameters,
        }
    }

    /// Record the result of a non-streamed call and end the observation.
    fn finish<T: Serialize>(self, result: &Result<T, OpenAIError>) {
        match result {
            Ok(response) => {
                if self.observation.is_recording() {
                    let response = to_json(response);
                    let output = match self.api {
                        Api::Chat => chat_output(&response),
                        Api::Responses => responses_output(&response["output"]),
                        Api::Embedding => embedding_output(&response),
                    };
                    self.record(
                        response["model"].as_str(),
                        output,
                        &response["usage"],
                        &response["service_tier"],
                    );
                }
                self.observation.end();
            }
            Err(error) => self.fail(error),
        }
    }

    fn record(&self, model: Option<&str>, output: Value, usage: &Value, service_tier: &Value) {
        let observation = &self.observation;
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            observation.set_model(model);
        }
        observation.set_output(&output);
        if let Some(usage) = parse_usage(usage) {
            observation.set_usage_details(&usage);
        }
        if !service_tier.is_null() {
            let mut parameters = self.model_parameters.clone();
            parameters.insert("service_tier".into(), service_tier.clone());
            observation.set_model_parameters(&parameters);
        }
    }

    fn fail(&self, error: &OpenAIError) {
        self.mark_failed(error);
        self.observation.end();
    }

    fn mark_failed(&self, error: &OpenAIError) {
        tracing::warn!(%error, "OpenAI call failed");
        let observation = &self.observation;
        observation.record_error(error);
        if let Some(model) = &self.model {
            observation.set_model(model.clone());
        }
        observation.set_cost_details(&json!({"input": 0, "output": 0, "total": 0}));
    }
}

/// A response stream that records its chunks on the call's observation.
struct TracedStream<T> {
    inner: async_openai::types::stream::StreamResponse<T>,
    call: Option<Call>,
    state: StreamState,
}

impl<T> TracedStream<T> {
    fn new(inner: async_openai::types::stream::StreamResponse<T>, call: Call) -> Self {
        let state = match call.api {
            Api::Responses => StreamState::Responses(ResponsesStream::default()),
            Api::Chat | Api::Embedding => StreamState::Chat(ChatStream::default()),
        };
        Self {
            inner,
            call: Some(call),
            state,
        }
    }

    fn finish(&mut self) {
        let Some(call) = self.call.take() else {
            return;
        };
        if call.observation.is_recording() {
            match &self.state {
                StreamState::Chat(chat) => {
                    if let Some(reason) = &chat.finish_reason {
                        call.observation
                            .set_metadata(&json!({"finish_reason": reason}));
                    }
                    call.record(
                        chat.model.as_deref(),
                        chat.output(),
                        &chat.usage,
                        &chat.service_tier,
                    );
                }
                StreamState::Responses(responses) => {
                    if !responses.metadata.is_empty() {
                        call.observation.set_metadata(&responses.metadata);
                    }
                    call.record(
                        responses.model.as_deref(),
                        responses.completion.clone(),
                        &responses.usage,
                        &responses.service_tier,
                    );
                }
            }
        }
        call.observation.end();
    }
}

impl<T> Drop for TracedStream<T> {
    fn drop(&mut self) {
        // Dropped before the end: record what was received.
        self.finish();
    }
}

impl<T: Serialize> Stream for TracedStream<T> {
    type Item = Result<T, OpenAIError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        // `inner` is a `Pin<Box<..>>`, so `Self` is `Unpin`.
        let this = self.get_mut();
        let polled = this.inner.as_mut().poll_next(cx);
        match &polled {
            Poll::Ready(Some(Ok(chunk))) => {
                if let Some(call) = &this.call
                    && call.observation.is_recording()
                {
                    if !this.state.started() {
                        call.observation.set_completion_start_time(Utc::now());
                    }
                    this.state.push(&to_json(chunk));
                }
            }
            Poll::Ready(Some(Err(error))) => {
                if let Some(call) = &this.call {
                    call.mark_failed(error);
                }
            }
            Poll::Ready(None) => this.finish(),
            Poll::Pending => {}
        }
        polled
    }
}

/// What a stream has delivered so far.
enum StreamState {
    Chat(ChatStream),
    Responses(ResponsesStream),
}

impl StreamState {
    fn started(&self) -> bool {
        match self {
            StreamState::Chat(chat) => chat.chunks > 0,
            StreamState::Responses(responses) => responses.events > 0,
        }
    }

    fn push(&mut self, chunk: &Value) {
        match self {
            StreamState::Chat(chat) => chat.push(chunk),
            StreamState::Responses(responses) => responses.push(chunk),
        }
    }
}

/// Accumulated chat completion chunks.
#[derive(Debug, Default)]
struct ChatStream {
    chunks: usize,
    model: Option<String>,
    service_tier: Value,
    usage: Value,
    finish_reason: Option<String>,
    role: Option<Value>,
    content: Option<String>,
    function_call: Option<Map<String, Value>>,
    tool_calls: Vec<Map<String, Value>>,
}

impl ChatStream {
    fn push(&mut self, chunk: &Value) {
        self.chunks += 1;
        if self.model.is_none() {
            self.model = chunk["model"]
                .as_str()
                .filter(|m| !m.is_empty())
                .map(str::to_owned);
        }
        if self.service_tier.is_null() {
            self.service_tier = chunk["service_tier"].clone();
        }
        if !chunk["usage"].is_null() {
            self.usage = chunk["usage"].clone();
        }
        for choice in chunk["choices"].as_array().into_iter().flatten() {
            if let Some(reason) = choice["finish_reason"].as_str() {
                self.finish_reason = Some(reason.to_owned());
            }
            let delta = &choice["delta"];
            if !delta["role"].is_null() {
                self.role = Some(delta["role"].clone());
            }
            if let Some(content) = delta["content"].as_str() {
                self.content
                    .get_or_insert_with(String::new)
                    .push_str(content);
            }
            if let Some(call) = delta["function_call"].as_object() {
                let current = self.function_call.get_or_insert_with(|| {
                    Map::from_iter([("name".into(), json!("")), ("arguments".into(), json!(""))])
                });
                merge_function(current, call);
            }
            for tool_call in delta["tool_calls"].as_array().into_iter().flatten() {
                let index = match tool_call["index"].as_u64() {
                    Some(index) => index as usize,
                    None => self.tool_calls.len().saturating_sub(1),
                };
                while self.tool_calls.len() <= index {
                    self.tool_calls.push(Map::from_iter([(
                        "function".into(),
                        json!({"name": "", "arguments": ""}),
                    )]));
                }
                let current = &mut self.tool_calls[index];
                for key in ["id", "type"] {
                    if !tool_call[key].is_null() {
                        current.insert(key.into(), tool_call[key].clone());
                    }
                }
                if let Some(function) = tool_call["function"].as_object()
                    && let Some(Value::Object(current)) = current.get_mut("function")
                {
                    merge_function(current, function);
                }
            }
        }
    }

    /// The assistant message: tool calls, else function call, else content.
    fn output(&self) -> Value {
        let with_content = |key: &str, value: Value| {
            let mut message = Map::from_iter([
                ("role".to_owned(), json!("assistant")),
                (key.to_owned(), value),
            ]);
            if let Some(content) = &self.content {
                message.insert("content".into(), json!(content));
            }
            Value::Object(message)
        };
        if !self.tool_calls.is_empty() {
            let calls = self.tool_calls.iter().cloned().map(Value::Object).collect();
            return with_content("tool_calls", Value::Array(calls));
        }
        if let Some(call) = &self.function_call {
            return with_content("function_call", Value::Object(call.clone()));
        }
        match &self.content {
            Some(content) if !content.is_empty() => json!(content),
            _ => Value::Null,
        }
    }
}

/// Append a streamed function-call fragment: the first name wins, arguments
/// are concatenated.
fn merge_function(current: &mut Map<String, Value>, fragment: &Map<String, Value>) {
    if let Some(name) = fragment.get("name").and_then(Value::as_str) {
        let empty = current
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty);
        if empty {
            current.insert("name".into(), json!(name));
        }
    }
    if let Some(arguments) = fragment.get("arguments").and_then(Value::as_str) {
        let mut all = current
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        all.push_str(arguments);
        current.insert("arguments".into(), json!(all));
    }
}

/// Accumulated Responses API events: the last one carrying a response wins.
#[derive(Debug, Default)]
struct ResponsesStream {
    events: usize,
    model: Option<String>,
    service_tier: Value,
    usage: Value,
    completion: Value,
    metadata: Map<String, Value>,
}

impl ResponsesStream {
    fn push(&mut self, event: &Value) {
        self.events += 1;
        let Some(response) = event["response"].as_object() else {
            return;
        };
        let usage = if event["usage"].is_null() {
            response.get("usage").cloned().unwrap_or_default()
        } else {
            event["usage"].clone()
        };
        if !usage.is_null() {
            self.usage = usage;
        }
        self.model = response
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(tier) = response.get("service_tier").filter(|t| !t.is_null()) {
            self.service_tier = tier.clone();
        }
        for (key, value) in response {
            match key.as_str() {
                "created_at" | "model" | "usage" | "text" => {}
                "output" => self.completion = responses_output(value),
                _ => {
                    self.metadata.insert(key.clone(), value.clone());
                }
            }
        }
    }
}

/// `value` as JSON, through its serialized text so `f32` fields keep their
/// shortest form (`0.2`, not `0.20000000298023224`).
fn to_json<T: Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_string(value)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

fn is_set(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

/// Model parameters of a request, with the Python SDK's defaults.
fn model_parameters(api: Api, request: &Value) -> Map<String, Value> {
    let get = |key: &str| is_set(request.get(key)).cloned();
    let mut parameters = Map::new();
    if api == Api::Embedding {
        if let Some(dimensions) = get("dimensions") {
            parameters.insert("dimensions".into(), dimensions);
        }
        if let Some(format) = get("encoding_format").filter(|f| f != "float") {
            parameters.insert("encoding_format".into(), format);
        }
        return parameters;
    }
    let defaults = [
        ("temperature", json!(1)),
        ("max_tokens", json!("Infinity")),
        ("top_p", json!(1)),
        ("frequency_penalty", json!(0)),
        ("presence_penalty", json!(0)),
    ];
    for (key, default) in defaults {
        parameters.insert(key.into(), get(key).unwrap_or(default));
    }
    if let Some(max) = get("max_completion_tokens") {
        parameters.remove("max_tokens");
        parameters.insert("max_completion_tokens".into(), max);
    }
    if let Some(n) = get("n").filter(|n| n.as_u64().is_some_and(|n| n > 1)) {
        parameters.insert("n".into(), n);
    }
    for key in ["seed", "service_tier"] {
        if let Some(value) = get(key) {
            parameters.insert(key.into(), value);
        }
    }
    let reasoning = match api {
        Api::Responses => [
            ("reasoning_effort", request.pointer("/reasoning/effort")),
            ("reasoning_summary", request.pointer("/reasoning/summary")),
            ("verbosity", request.pointer("/text/verbosity")),
            ("max_output_tokens", request.get("max_output_tokens")),
        ]
        .to_vec(),
        _ => [
            ("reasoning_effort", request.get("reasoning_effort")),
            ("verbosity", request.get("verbosity")),
        ]
        .to_vec(),
    };
    for (key, value) in reasoning {
        if let Some(value) = is_set(value) {
            parameters.insert(key.into(), value.clone());
        }
    }
    if parameters.contains_key("max_output_tokens") {
        parameters.remove("max_tokens");
    }
    parameters
}

/// Chat input: the messages, or an object with the tools offered too.
fn chat_input(request: &Value) -> Value {
    let messages: Vec<Value> = request["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(process_message)
        .collect();
    let mut prompt = Map::new();
    for key in ["functions", "function_call", "tools"] {
        if let Some(value) = is_set(request.get(key)) {
            prompt.insert(key.into(), value.clone());
        }
    }
    if prompt.is_empty() {
        return Value::Array(messages);
    }
    prompt.insert("messages".into(), Value::Array(messages));
    Value::Object(prompt)
}

/// Turn the base64 audio of `input_audio` parts into data URIs (uploaded as
/// media by the tracer).
fn process_message(message: &Value) -> Value {
    let mut message = message.clone();
    if let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) {
        for part in parts {
            if part["type"] != "input_audio" {
                continue;
            }
            let audio = &mut part["input_audio"];
            let format = audio["format"].as_str().unwrap_or("wav").to_owned();
            if let Some(data) = audio["data"].as_str() {
                audio["data"] = json!(format!("data:audio/{format};base64,{data}"));
            }
        }
    }
    message
}

/// Responses API input: `instructions` as a system message before `input`,
/// plus the tool fields.
fn responses_input(request: &Value) -> Value {
    let input = is_set(request.get("input")).cloned();
    let instructions = is_set(request.get("instructions")).cloned();
    let prompt = match (instructions, input) {
        (None, input) => input,
        (Some(instructions), None) => Some(json!({"instructions": instructions})),
        (Some(instructions), Some(Value::String(input))) => Some(json!([
            {"role": "system", "content": instructions},
            {"role": "user", "content": input},
        ])),
        (Some(instructions), Some(Value::Array(items))) => {
            let system = json!({"role": "system", "content": instructions});
            Some(Value::Array(std::iter::once(system).chain(items).collect()))
        }
        (Some(instructions), Some(input)) => {
            Some(json!({"instructions": instructions, "input": input}))
        }
    };
    let mut fields = Map::new();
    for key in ["tools", "tool_choice", "parallel_tool_calls"] {
        if let Some(value) = is_set(request.get(key)) {
            fields.insert(key.into(), value.clone());
        }
    }
    if fields.is_empty() {
        return prompt.unwrap_or(Value::Null);
    }
    match prompt {
        Some(Value::Object(mut object))
            if object.keys().all(|k| k == "instructions" || k == "input") =>
        {
            object.extend(fields);
            Value::Object(object)
        }
        Some(prompt) => {
            fields.insert("input".into(), prompt);
            Value::Object(fields)
        }
        None => Value::Object(fields),
    }
}

/// Chat output: the message of the only choice, or the list of messages.
fn chat_output(response: &Value) -> Value {
    let choices = response["choices"].as_array().cloned().unwrap_or_default();
    let mut messages: Vec<Value> = choices
        .iter()
        .map(|choice| chat_message(&choice["message"]))
        .collect();
    match messages.len() {
        0 => Value::Null,
        1 => messages.remove(0),
        _ => Value::Array(messages),
    }
}

/// `{role, function_call?, tool_calls?, content, audio?}` of a message, with
/// audio data as a data URI.
fn chat_message(message: &Value) -> Value {
    let mut out = Map::new();
    out.insert("role".into(), message["role"].clone());
    for key in ["function_call", "tool_calls"] {
        if let Some(value) = is_set(message.get(key)) {
            out.insert(key.into(), value.clone());
        }
    }
    out.insert("content".into(), message["content"].clone());
    if let Some(Value::Object(audio)) = is_set(message.get("audio")) {
        let mut audio = audio.clone();
        if let Some(data) = audio.get("data").and_then(Value::as_str) {
            let format = audio
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("wav")
                .to_owned();
            let uri = format!("data:audio/{format};base64,{data}");
            audio.insert("data".into(), json!(uri));
        }
        out.insert("audio".into(), Value::Object(audio));
    }
    Value::Object(out)
}

/// Responses API output: the only item, or the list.
fn responses_output(output: &Value) -> Value {
    match output {
        Value::Array(items) if items.len() == 1 => items[0].clone(),
        Value::Array(items) if items.is_empty() => Value::Null,
        other => other.clone(),
    }
}

/// `{dimensions, count}` of an embedding response.
fn embedding_output(response: &Value) -> Value {
    let data = response["data"].as_array().cloned().unwrap_or_default();
    let dimensions = data
        .first()
        .and_then(|first| first["embedding"].as_array())
        .map_or(0, Vec::len);
    json!({"dimensions": dimensions, "count": data.len()})
}

/// Usage without null values; embedding usage (`prompt_tokens` and
/// `total_tokens` only) becomes `{"input": n}`.
fn parse_usage(usage: &Value) -> Option<Value> {
    fn without_nulls(value: &Value) -> Value {
        match value {
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k.clone(), without_nulls(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    let usage = without_nulls(usage);
    let object = usage.as_object()?;
    if object.len() == 2
        && object.contains_key("prompt_tokens")
        && object.contains_key("total_tokens")
    {
        return Some(json!({"input": object["prompt_tokens"]}));
    }
    (!object.is_empty()).then_some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_model_parameters_have_python_defaults() {
        let request = json!({"model": "gpt-4o", "messages": [], "temperature": 0.2, "n": 1});
        assert_eq!(
            Value::Object(model_parameters(Api::Chat, &request)),
            json!({"temperature": 0.2, "max_tokens": "Infinity", "top_p": 1,
                   "frequency_penalty": 0, "presence_penalty": 0})
        );
        let request = json!({"max_completion_tokens": 50, "n": 2, "seed": 7,
                             "service_tier": "flex", "reasoning_effort": "low", "verbosity": "high"});
        let parameters = model_parameters(Api::Chat, &request);
        assert!(!parameters.contains_key("max_tokens"));
        for (key, value) in [
            ("max_completion_tokens", json!(50)),
            ("n", json!(2)),
            ("seed", json!(7)),
            ("service_tier", json!("flex")),
            ("reasoning_effort", json!("low")),
            ("verbosity", json!("high")),
        ] {
            assert_eq!(parameters[key], value, "{key}");
        }
    }

    #[test]
    fn responses_and_embedding_model_parameters() {
        let request = json!({"reasoning": {"effort": "high", "summary": "auto"},
                             "text": {"verbosity": "low"}, "max_output_tokens": 300});
        let parameters = model_parameters(Api::Responses, &request);
        assert!(!parameters.contains_key("max_tokens"));
        assert_eq!(parameters["reasoning_effort"], "high");
        assert_eq!(parameters["reasoning_summary"], "auto");
        assert_eq!(parameters["verbosity"], "low");
        assert_eq!(parameters["max_output_tokens"], 300);

        let embedding = json!({"dimensions": 256, "encoding_format": "float"});
        assert_eq!(
            Value::Object(model_parameters(Api::Embedding, &embedding)),
            json!({"dimensions": 256})
        );
        let embedding = json!({"encoding_format": "base64"});
        assert_eq!(
            Value::Object(model_parameters(Api::Embedding, &embedding)),
            json!({"encoding_format": "base64"})
        );
    }

    #[test]
    fn chat_input_includes_tools_and_audio_as_data_uris() {
        let messages = json!([{"role": "user", "content": [
            {"type": "text", "text": "Transcribe"},
            {"type": "input_audio", "input_audio": {"data": "UklGRg==", "format": "mp3"}}
        ]}]);
        let input = chat_input(&json!({"messages": messages}));
        assert_eq!(
            input[0]["content"][1]["input_audio"]["data"],
            "data:audio/mp3;base64,UklGRg=="
        );
        let tools = json!([{"type": "function", "function": {"name": "f"}}]);
        let input = chat_input(&json!({"messages": [], "tools": tools, "functions": null}));
        assert_eq!(input, json!({"tools": tools, "messages": []}));
    }

    #[test]
    fn responses_input_puts_instructions_first() {
        let cases = [
            (json!({"input": "Hi"}), json!("Hi")),
            (
                json!({"instructions": "Be brief"}),
                json!({"instructions": "Be brief"}),
            ),
            (
                json!({"instructions": "Be brief", "input": "Hi"}),
                json!([{"role": "system", "content": "Be brief"}, {"role": "user", "content": "Hi"}]),
            ),
            (
                json!({"instructions": "Be brief", "input": [{"role": "user", "content": "Hi"}]}),
                json!([{"role": "system", "content": "Be brief"}, {"role": "user", "content": "Hi"}]),
            ),
            (
                json!({"input": "Hi", "tool_choice": "auto"}),
                json!({"input": "Hi", "tool_choice": "auto"}),
            ),
            (
                json!({"instructions": "Be brief", "tools": []}),
                json!({"instructions": "Be brief", "tools": []}),
            ),
        ];
        for (request, expected) in cases {
            assert_eq!(responses_input(&request), expected, "{request}");
        }
    }

    #[test]
    fn outputs_and_usage() {
        let response = json!({"choices": [{"message": {"role": "assistant", "content": "Hi",
            "tool_calls": null, "audio": {"id": "a", "data": "AAAA", "transcript": "Hi"}}}]});
        assert_eq!(
            chat_output(&response),
            json!({"role": "assistant", "content": "Hi",
                   "audio": {"id": "a", "data": "data:audio/wav;base64,AAAA", "transcript": "Hi"}})
        );
        let two = json!({"choices": [{"message": {"role": "assistant", "content": "a"}},
                                     {"message": {"role": "assistant", "content": "b"}}]});
        assert_eq!(chat_output(&two).as_array().unwrap().len(), 2);
        assert_eq!(
            embedding_output(
                &json!({"data": [{"embedding": [0.1, 0.2]}, {"embedding": [0.3, 0.4]}]})
            ),
            json!({"dimensions": 2, "count": 2})
        );
        assert_eq!(
            responses_output(&json!([{"type": "message"}])),
            json!({"type": "message"})
        );
        assert_eq!(responses_output(&json!([])), Value::Null);

        assert_eq!(
            parse_usage(&json!({"prompt_tokens": 5, "total_tokens": 5})),
            Some(json!({"input": 5}))
        );
        assert_eq!(
            parse_usage(
                &json!({"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7,
                "prompt_tokens_details": {"cached_tokens": 1, "audio_tokens": null},
                "completion_tokens_details": null})
            ),
            Some(
                json!({"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7,
                "prompt_tokens_details": {"cached_tokens": 1}})
            )
        );
        assert_eq!(parse_usage(&Value::Null), None);
    }

    #[test]
    fn chat_streams_accumulate_content_and_tool_calls() {
        let mut stream = ChatStream::default();
        let chunks = [
            json!({"model": "gpt-4o-2024", "choices": [{"delta": {"role": "assistant", "tool_calls": [
                {"index": 0, "id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "\"x\"}"}},
                {"index": 1, "id": "call_2", "function": {"name": "other", "arguments": "{}"}}]}}]}),
            json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}],
                   "usage": {"prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7}}),
        ];
        for chunk in &chunks {
            stream.push(chunk);
        }
        assert_eq!(stream.model.as_deref(), Some("gpt-4o-2024"));
        assert_eq!(stream.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(stream.usage["total_tokens"], 7);
        assert_eq!(
            stream.output(),
            json!({"role": "assistant", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}},
                {"id": "call_2", "function": {"name": "other", "arguments": "{}"}}
            ]})
        );

        let mut text = ChatStream::default();
        for content in ["Hel", "lo"] {
            text.push(&json!({"choices": [{"delta": {"content": content}}]}));
        }
        assert_eq!(text.output(), json!("Hello"));
        assert_eq!(ChatStream::default().output(), Value::Null);
    }

    #[test]
    fn responses_streams_keep_the_last_response() {
        let mut stream = ResponsesStream::default();
        stream.push(&json!({"type": "response.output_text.delta", "delta": "Hi"}));
        stream.push(&json!({"type": "response.completed", "response": {
            "id": "resp_1", "model": "gpt-4.1", "status": "completed", "created_at": 1,
            "output": [{"type": "message", "content": [{"type": "output_text", "text": "Hi"}]}],
            "usage": {"input_tokens": 3, "output_tokens": 1, "total_tokens": 4},
            "service_tier": "default", "text": {"format": {"type": "text"}}}}));
        assert_eq!(stream.model.as_deref(), Some("gpt-4.1"));
        assert_eq!(stream.usage["total_tokens"], 4);
        assert_eq!(stream.completion["type"], "message");
        assert_eq!(stream.service_tier, "default");
        assert_eq!(stream.metadata["id"], "resp_1");
        assert!(!stream.metadata.contains_key("created_at"));
        assert!(!stream.metadata.contains_key("text"));
    }
}
