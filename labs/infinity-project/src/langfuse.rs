//! Langfuse tracing of the chat (<https://langfuse.com>), with
//! `langfuse-rust-sdk`.
//!
//! Every turn is one trace, and the turns of one conversation share a
//! session (`/clear` starts a new one):
//!
//! - the turn is an agent observation, with the message as input and the
//!   reply as output;
//! - under it, each call to the model is a generation, recorded by a wrapper
//!   around the model provider: the request as it was sent (system prompt,
//!   messages, tools), the output and the token usage;
//! - and each tool the model calls is a tool observation, with its arguments
//!   and its result, from the generation that calls it to the model call
//!   that receives the result.
//!
//! The credentials are read from the environment (`LANGFUSE_PUBLIC_KEY`,
//! `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL`), which `main` fills from
//! `.env`.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, ready};
use std::time::SystemTime;

use async_trait::async_trait;
use futures_core::Stream;
use infinity_provider_protocol::{
    AssistantContent, CompletionError, CompletionRequest, Message, ModelEntry, ModelProvider,
    ModelStream, Reasoning, StreamChunk, ToolCall, ToolResult, ToolResultContent, Usage,
    UserContent,
};
use langfuse_rust_sdk::otel::{
    LangfuseTracer, Level, Observation, ObservationType, PropagatedAttributes, TracingOptions,
    propagate_attributes,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, ids};
use serde_json::{Map, Value, json};

use crate::usage::Tokens;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const PUBLIC_KEY: &str = "LANGFUSE_PUBLIC_KEY";
const SECRET_KEY: &str = "LANGFUSE_SECRET_KEY";
/// Name of the turn observations and of their traces.
const TURN_NAME: &str = "chat-turn";
const GENERATION_NAME: &str = "completion";
const TAG: &str = "infinity-project";

/// The connection to Langfuse of one run of the chat.
pub struct Langfuse {
    tracer: LangfuseTracer,
    shared: Shared,
    /// Where the Langfuse UI shows a session, up to the session's ID.
    sessions_url: String,
}

impl Langfuse {
    /// Connects with the credentials in the environment and checks them. The
    /// error says why the chat is not traced.
    pub async fn connect() -> Result<Self, BoxError> {
        let is_set = |name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty());
        if !is_set(PUBLIC_KEY) && !is_set(SECRET_KEY) {
            return Err(
                format!("{PUBLIC_KEY} and {SECRET_KEY} are not set (environment or .env)").into(),
            );
        }
        let config = ClientConfig::from_env()?;
        let base_url = config.base_url().as_str().trim_end_matches('/').to_owned();
        let client = LangfuseClient::new(config)?;
        // Wrong keys must show now: the spans are exported in the background,
        // where a rejected export is only a log line.
        let project = client.auth_check().await?;
        let tracer = client.tracer(TracingOptions::from_env()?)?;
        if !tracer.is_enabled() {
            return Err("tracing is disabled in the environment".into());
        }

        let langfuse = Self {
            tracer,
            shared: Shared::default(),
            sessions_url: format!("{base_url}/project/{}/sessions/", project.id),
        };
        langfuse.new_session();
        Ok(langfuse)
    }

    /// Starts a session: the traces from now on belong to a new
    /// conversation.
    pub fn new_session(&self) {
        let session_id = format!("chat-{}", ids::create_observation_id(None));
        let mut state = self.shared.lock();
        state.session_url = format!("{}{session_id}", self.sessions_url);
        state.attributes = PropagatedAttributes::default()
            .with_session_id(session_id)
            .with_trace_name(TURN_NAME)
            .with_tags([TAG]);
    }

    /// Where the Langfuse UI shows the traces of the current session.
    pub fn session_url(&self) -> String {
        self.shared.lock().session_url.clone()
    }

    /// Wraps `provider` so that every call to a model is recorded as a
    /// generation, and every tool the model calls as a tool observation.
    pub fn trace_provider(&self, provider: Arc<dyn ModelProvider>) -> Arc<dyn ModelProvider> {
        Arc::new(TracedProvider {
            inner: provider,
            tracer: self.tracer.clone(),
            shared: self.shared.clone(),
        })
    }

    /// Starts the trace of a turn. It ends when the returned value is
    /// dropped.
    pub async fn start_turn(&self, message: &str) -> TurnTrace {
        let attributes = self.shared.attributes();
        let root = start_root(&self.tracer, attributes, TURN_NAME, ObservationType::Agent).await;
        root.set_input(message);
        self.shared.lock().turn = Some(root.clone());
        TurnTrace {
            root,
            shared: self.shared.clone(),
            reply: String::new(),
        }
    }

    /// Sends what is still queued. Call it before the chat exits.
    pub async fn shutdown(&self) {
        self.shared.end_tools();
        self.tracer.shutdown().await;
    }
}

/// Starts an observation that is the root of a new trace of the run.
async fn start_root(
    tracer: &LangfuseTracer,
    attributes: PropagatedAttributes,
    name: &str,
    kind: ObservationType,
) -> Observation {
    // The observation keeps the attributes of the scope it starts in, and
    // hands them on to its children.
    propagate_attributes(attributes, async { tracer.start_observation(name, kind) }).await
}

/// What the chat and the provider share. The model is called from the
/// thread's task, where no tracing context of the chat's task applies, so
/// this is how the provider finds the turn its observations belong to.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    /// Session, trace name and tags of the traces of the current session.
    attributes: PropagatedAttributes,
    /// Where the Langfuse UI shows the current session.
    session_url: String,
    /// The observation of the turn in progress.
    turn: Option<Observation>,
    /// The tool calls the model has no result for yet, by call ID.
    tools: HashMap<String, Observation>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn turn(&self) -> Option<Observation> {
        self.lock().turn.clone()
    }

    fn attributes(&self) -> PropagatedAttributes {
        self.lock().attributes.clone()
    }

    /// A generation called a tool: it runs from now until the model is
    /// called with its result.
    fn tool_called(&self, generation: &Observation, call: &ToolCall) {
        let mut state = self.lock();
        // Next to the generations of its turn, in the order things happened.
        let parent = state.turn.as_ref().unwrap_or(generation);
        let tool = parent.start_child(call.function.name.as_str(), ObservationType::Tool);
        tool.set_input(&call.function.arguments);
        state.tools.insert(call.id.clone(), tool);
    }

    /// The model is being called with `result`.
    fn tool_answered(&self, result: &ToolResult) {
        if let Some(tool) = self.lock().tools.remove(&result.id) {
            tool.set_output(&tool_result_text(result));
            tool.end();
        }
    }

    /// Ends the tool calls still without a result: nothing is waiting on
    /// them any more.
    fn end_tools(&self) {
        for (_, tool) in self.lock().tools.drain() {
            tool.end();
        }
    }
}

/// The trace of one turn. Dropping it ends the turn's observation, with the
/// reply so far as output.
pub struct TurnTrace {
    root: Observation,
    shared: Shared,
    /// The text of the reply so far.
    reply: String,
}

impl TurnTrace {
    /// A chunk of the reply.
    pub fn reply_chunk(&mut self, text: &str) {
        self.reply.push_str(text);
    }

    /// The model called a tool: what it says next is a new paragraph of the
    /// reply.
    pub fn reply_break(&mut self) {
        if !self.reply.is_empty() && !self.reply.ends_with('\n') {
            self.reply.push_str("\n\n");
        }
    }

    /// The turn reported an error.
    pub fn error(&mut self, message: &str) {
        self.root.record_error(message);
    }

    /// The user left with Ctrl+C before the turn was over.
    pub fn interrupted(&mut self) {
        self.root
            .set_level(Level::Warning)
            .set_status_message("interrupted by the user");
    }
}

impl Drop for TurnTrace {
    fn drop(&mut self) {
        self.shared.lock().turn = None;
        // A turn can end on a tool call: the model waiting for the next
        // message. That wait is not part of the turn.
        self.shared.end_tools();
        let reply = self.reply.trim();
        if !reply.is_empty() {
            self.root.set_output(reply);
        }
        self.root.end();
    }
}

/// A model provider that records every call as a Langfuse generation, and
/// the tool calls between two of them as tool observations.
struct TracedProvider {
    inner: Arc<dyn ModelProvider>,
    tracer: LangfuseTracer,
    shared: Shared,
}

#[async_trait]
impl ModelProvider for TracedProvider {
    async fn list_models(&self) -> Result<Vec<ModelEntry>, BoxError> {
        self.inner.list_models().await
    }

    async fn invoke_model(
        &self,
        model_id: &str,
        request: CompletionRequest,
    ) -> Result<ModelStream, CompletionError> {
        // The results at the end of the conversation are what this call is
        // made for: their tools are done.
        for result in trailing_tool_results(&request.chat_history) {
            self.shared.tool_answered(result);
        }

        let observation = match self.shared.turn() {
            Some(turn) => turn.start_child(GENERATION_NAME, ObservationType::Generation),
            // The thread also calls the model on its own, between turns
            // (a timer it set, a compaction): a trace of its own.
            None => {
                let attributes = self.shared.attributes();
                let kind = ObservationType::Generation;
                start_root(&self.tracer, attributes, GENERATION_NAME, kind).await
            }
        };
        observation
            .set_model(model_id)
            .set_input(&request_input(&request));
        if let Some(parameters) = model_parameters(&request) {
            observation.set_model_parameters(&parameters);
        }

        let mut generation = Generation::new(observation, self.shared.clone());
        match self.inner.invoke_model(model_id, request).await {
            Ok(stream) => Ok(Box::pin(TracedStream { stream, generation })),
            Err(error) => {
                generation.fail(&error);
                Err(error)
            }
        }
    }
}

/// A completion stream that feeds its chunks to a [`Generation`] on their way
/// to the caller.
struct TracedStream {
    stream: ModelStream,
    generation: Generation,
}

impl Stream for TracedStream {
    type Item = Result<StreamChunk, CompletionError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let item = ready!(this.stream.as_mut().poll_next(cx));
        match &item {
            Some(Ok(chunk)) => this.generation.chunk(chunk),
            Some(Err(error)) => this.generation.fail(error),
            None => this.generation.finish(),
        }
        Poll::Ready(item)
    }
}

/// The generation of one model call: collects the output while it streams,
/// and ends the observation with the stream's final chunk, or when the stream
/// fails or is dropped (a cancelled call keeps what it had produced).
struct Generation {
    observation: Observation,
    shared: Shared,
    /// The content blocks of the output that are complete.
    parts: Vec<Value>,
    /// The text since the last complete block.
    text: String,
    /// Whether the model has started to answer.
    started: bool,
    /// Whether the output has called a tool.
    called_tool: bool,
}

impl Generation {
    fn new(observation: Observation, shared: Shared) -> Self {
        Self {
            observation,
            shared,
            parts: Vec::new(),
            text: String::new(),
            started: false,
            called_tool: false,
        }
    }

    fn chunk(&mut self, chunk: &StreamChunk) {
        if !self.started {
            self.started = true;
            self.observation
                .set_completion_start_time(SystemTime::now());
        }
        match chunk {
            StreamChunk::Text(text) => self.text.push_str(text),
            StreamChunk::ToolCall(call) => {
                self.push_part(tool_call_part(call));
                // The runtime runs the first tool a response calls, as soon
                // as the call is complete, and ignores the others.
                if !self.called_tool {
                    self.called_tool = true;
                    self.shared.tool_called(&self.observation, call);
                }
            }
            StreamChunk::Reasoning(reasoning) => self.push_part(reasoning_part(reasoning)),
            // The last chunk of a stream. The runtime stops reading here,
            // and may only drop the stream once it has called the model
            // again, so this is where the generation ends.
            StreamChunk::Final(response) => {
                if let Some(usage) = &response.usage {
                    self.observation.set_usage_details(&usage_details(usage));
                }
                self.finish();
            }
            // The complete block follows its deltas.
            StreamChunk::ToolCallDelta { .. } | StreamChunk::ReasoningDelta { .. } => {}
        }
    }

    fn push_part(&mut self, part: Value) {
        self.flush_text();
        self.parts.push(part);
    }

    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            self.parts.push(text_part(&std::mem::take(&mut self.text)));
        }
    }

    fn fail(&mut self, error: &CompletionError) {
        self.observation.record_error(error);
        self.finish();
    }

    /// Records the output and ends the observation. Later calls do nothing.
    fn finish(&mut self) {
        self.flush_text();
        if !self.parts.is_empty() {
            let content = content_json(std::mem::take(&mut self.parts));
            self.observation
                .set_output(&json!({"role": "assistant", "content": content}));
        }
        self.observation.end();
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The request as chat messages, the system prompt first, with the tool
/// definitions when there are any: the shape the SDK's own model integrations
/// record.
fn request_input(request: &CompletionRequest) -> Value {
    let mut messages: Vec<Value> = request
        .preamble
        .iter()
        .map(|system| json!({"role": "system", "content": system}))
        .collect();
    messages.extend(request.chat_history.iter().map(message_json));
    if request.tools.is_empty() {
        Value::Array(messages)
    } else {
        json!({"tools": request.tools, "messages": messages})
    }
}

fn message_json(message: &Message) -> Value {
    let (role, parts): (&str, Vec<Value>) = match message {
        Message::User { content } => ("user", content.iter().map(user_part).collect()),
        Message::Assistant { content } => {
            ("assistant", content.iter().map(assistant_part).collect())
        }
    };
    json!({"role": role, "content": content_json(parts)})
}

/// The content of a message: plain text when that is all it is, else its
/// blocks.
fn content_json(mut parts: Vec<Value>) -> Value {
    match parts.as_mut_slice() {
        [part] if part["type"] == "text" => part["text"].take(),
        _ => Value::Array(parts),
    }
}

fn user_part(part: &UserContent) -> Value {
    match part {
        UserContent::Text(text) => text_part(&text.text),
        UserContent::ToolResult(result) => json!({
            "type": "tool_result",
            "tool_use_id": result.id,
            "content": tool_result_text(result),
        }),
        UserContent::Image(_) => json!({"type": "image"}),
    }
}

fn assistant_part(part: &AssistantContent) -> Value {
    match part {
        AssistantContent::Text(text) => text_part(&text.text),
        AssistantContent::ToolCall(call) => tool_call_part(call),
        AssistantContent::Reasoning(reasoning) => reasoning_part(reasoning),
    }
}

fn text_part(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

fn tool_call_part(call: &ToolCall) -> Value {
    json!({
        "type": "tool_use",
        "id": call.id,
        "name": call.function.name,
        "input": call.function.arguments,
    })
}

/// The readable text of a reasoning block, without its signature.
fn reasoning_part(reasoning: &Reasoning) -> Value {
    json!({"type": "thinking", "thinking": reasoning.display_text()})
}

/// The tool results that end the conversation: what the model is called
/// with after its tools have run.
fn trailing_tool_results(history: &[Message]) -> impl Iterator<Item = &ToolResult> {
    history
        .iter()
        .rev()
        .map_while(|message| match message {
            Message::User { content } => Some(content),
            Message::Assistant { .. } => None,
        })
        .flatten()
        .filter_map(|part| match part {
            UserContent::ToolResult(result) => Some(result),
            _ => None,
        })
}

fn tool_result_text(result: &ToolResult) -> String {
    let texts: Vec<&str> = result
        .content
        .iter()
        .map(|content| match content {
            ToolResultContent::Text(text) => text.text.as_str(),
            ToolResultContent::Image(_) => "[image]",
        })
        .collect();
    texts.join("\n")
}

/// `max_tokens` and the provider-specific parameters of the request, if any.
fn model_parameters(request: &CompletionRequest) -> Option<Map<String, Value>> {
    let mut parameters = match &request.additional_params {
        Some(Value::Object(extra)) => extra.clone(),
        _ => Map::new(),
    };
    if let Some(max_tokens) = request.max_tokens {
        parameters.insert("max_tokens".to_owned(), max_tokens.into());
    }
    (!parameters.is_empty()).then_some(parameters)
}

/// Token usage under the keys Langfuse prices: the input that did not go
/// through the cache, and cache reads and writes on their own.
fn usage_details(usage: &Usage) -> Map<String, Value> {
    let tokens = Tokens::of(usage);
    let mut details = Map::new();
    details.insert("input".to_owned(), tokens.uncached_input.into());
    details.insert("output".to_owned(), tokens.output.into());
    details.insert("total".to_owned(), tokens.total().into());
    if tokens.cache_read > 0 {
        details.insert("input_cache_read".to_owned(), tokens.cache_read.into());
    }
    if tokens.cache_write > 0 {
        details.insert("input_cache_creation".to_owned(), tokens.cache_write.into());
    }
    details
}

#[cfg(test)]
mod tests {
    use infinity_provider_protocol::{
        AssistantContent, CompletionRequest, Message, Text, ToolCall, ToolDefinition, ToolFunction,
        ToolResult, ToolResultContent, Usage, UserContent,
    };
    use serde_json::json;

    use super::{model_parameters, request_input, trailing_tool_results, usage_details};

    fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".to_owned(),
            call_id: None,
            function: ToolFunction {
                name: "list_directory".to_owned(),
                arguments: json!({"path": "src"}),
            },
        }
    }

    #[test]
    fn input_is_the_conversation_with_the_system_prompt_first() {
        let request = CompletionRequest {
            preamble: Some("Be brief.".to_owned()),
            chat_history: vec![Message::user("Hi"), Message::assistant("Hello.")],
            ..CompletionRequest::default()
        };
        assert_eq!(
            request_input(&request),
            json!([
                {"role": "system", "content": "Be brief."},
                {"role": "user", "content": "Hi"},
                {"role": "assistant", "content": "Hello."},
            ])
        );
    }

    #[test]
    fn input_carries_tools_tool_calls_and_tool_results() {
        let request = CompletionRequest {
            chat_history: vec![
                Message::Assistant {
                    content: vec![
                        AssistantContent::text("Looking."),
                        AssistantContent::ToolCall(tool_call()),
                    ],
                },
                Message::User {
                    content: vec![UserContent::ToolResult(ToolResult {
                        id: "call-1".to_owned(),
                        call_id: None,
                        content: vec![ToolResultContent::Text(Text {
                            text: "main.rs".to_owned(),
                        })],
                    })],
                },
            ],
            tools: vec![ToolDefinition {
                name: "list_directory".to_owned(),
                description: "Lists a directory.".to_owned(),
                parameters: json!({"type": "object"}),
            }],
            ..CompletionRequest::default()
        };
        assert_eq!(
            request_input(&request),
            json!({
                "tools": [{
                    "name": "list_directory",
                    "description": "Lists a directory.",
                    "parameters": {"type": "object"},
                }],
                "messages": [
                    {"role": "assistant", "content": [
                        {"type": "text", "text": "Looking."},
                        {
                            "type": "tool_use",
                            "id": "call-1",
                            "name": "list_directory",
                            "input": {"path": "src"},
                        },
                    ]},
                    {"role": "user", "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call-1",
                        "content": "main.rs",
                    }]},
                ],
            })
        );
    }

    #[test]
    fn only_the_results_after_the_last_assistant_message_are_new() {
        let result = |id: &str| {
            UserContent::ToolResult(ToolResult {
                id: id.to_owned(),
                call_id: None,
                content: Vec::new(),
            })
        };
        let history = [
            Message::User {
                content: vec![result("old")],
            },
            Message::assistant("Two more."),
            Message::User {
                content: vec![result("a"), UserContent::text("and a remark")],
            },
            Message::User {
                content: vec![result("b")],
            },
        ];
        let mut ids: Vec<&str> = trailing_tool_results(&history)
            .map(|result| result.id.as_str())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(trailing_tool_results(&history[..2]).count(), 0);
    }

    #[test]
    fn parameters_merge_max_tokens_with_the_provider_ones() {
        let mut request = CompletionRequest::default();
        assert_eq!(model_parameters(&request), None);

        request.max_tokens = Some(1024);
        request.additional_params = Some(json!({"thinking": {"type": "adaptive"}}));
        assert_eq!(
            json!(model_parameters(&request)),
            json!({"max_tokens": 1024, "thinking": {"type": "adaptive"}})
        );
    }

    #[test]
    fn usage_separates_cache_reads_and_writes_from_the_input() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 20,
            total_tokens: 330,
            cached_input_tokens: 200,
        };
        assert_eq!(
            json!(usage_details(&usage)),
            json!({
                "input": 10,
                "output": 20,
                "total": 330,
                "input_cache_read": 200,
                "input_cache_creation": 100,
            })
        );

        let uncached = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cached_input_tokens: 0,
        };
        assert_eq!(
            json!(usage_details(&uncached)),
            json!({"input": 100, "output": 50, "total": 150})
        );
    }
}
