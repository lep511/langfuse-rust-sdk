//! Langfuse observations on top of OpenTelemetry spans.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use opentelemetry::baggage::BaggageExt;
use opentelemetry::context::FutureExt;
use opentelemetry::trace::Span as _;
use opentelemetry::trace::{
    SpanContext, SpanId, SpanRef, Status, TraceContextExt, TraceFlags, TraceId, TraceState, Tracer,
};
use opentelemetry::{Context, KeyValue, Value as OtelValue};
use serde::Serialize;

use super::attributes::{self as attr, flatten_metadata, io_value, js_safe, to_json};
use super::propagation::{PromptLink, PropagatedState, TRACE_ID_BAGGAGE_KEY};
use super::tracer::TracerShared;
use crate::models::prompts::Prompt;
use crate::models::scores::CreateScoreRequest;

/// Recorded instead of data whose mask function panicked.
const MASK_FALLBACK: &str = "<fully masked due to failed mask function>";

/// Kind of a Langfuse observation, shown and filterable in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObservationType {
    /// A generic unit of work.
    Span,
    /// An LLM call: model, usage, cost and prompt can be recorded.
    Generation,
    /// A point in time; ends where it starts.
    Event,
    /// An agent deciding on and running steps.
    Agent,
    /// A tool call.
    Tool,
    /// A chain of steps.
    Chain,
    /// A retrieval step (e.g. vector search).
    Retriever,
    /// An embedding call; records the same fields as a generation.
    Embedding,
    /// An evaluation (e.g. LLM-as-a-judge).
    Evaluator,
    /// A guardrail check.
    Guardrail,
}

impl ObservationType {
    /// Wire name (`span`, `generation`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            ObservationType::Span => "span",
            ObservationType::Generation => "generation",
            ObservationType::Event => "event",
            ObservationType::Agent => "agent",
            ObservationType::Tool => "tool",
            ObservationType::Chain => "chain",
            ObservationType::Retriever => "retriever",
            ObservationType::Embedding => "embedding",
            ObservationType::Evaluator => "evaluator",
            ObservationType::Guardrail => "guardrail",
        }
    }

    /// Whether model, usage, cost, completion start time and prompt apply.
    pub fn is_generation_like(self) -> bool {
        matches!(
            self,
            ObservationType::Generation | ObservationType::Embedding
        )
    }
}

/// Severity of an observation. [`Level::Error`] also marks the span as failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// Verbose detail.
    Debug,
    /// The default level.
    Default,
    /// Something unexpected that did not fail.
    Warning,
    /// A failure.
    Error,
}

impl Level {
    /// Wire name (`DEBUG`, `DEFAULT`, `WARNING`, `ERROR`).
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Default => "DEFAULT",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
        }
    }
}

/// An existing trace (and optionally parent observation) to attach new
/// observations to, e.g. from another service or a stored ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// Trace ID: 32 lowercase hex characters (see [`crate::ids::create_trace_id`]).
    pub trace_id: String,
    /// Parent observation ID: 16 lowercase hex characters. Without it the new
    /// observation becomes the root of the trace.
    pub parent_observation_id: Option<String>,
}

impl TraceContext {
    /// A context for trace `trace_id`, without parent observation.
    pub fn new(trace_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            parent_observation_id: None,
        }
    }

    /// The remote parent span context, or `None` (with a warning) for an
    /// invalid trace ID. An invalid parent ID is ignored.
    pub(crate) fn span_context(&self) -> Option<SpanContext> {
        let trace_id = parse_hex_id(&self.trace_id, 32)
            .and_then(|hex| TraceId::from_hex(hex).ok())
            .filter(|id| *id != TraceId::INVALID);
        let Some(trace_id) = trace_id else {
            tracing::warn!(trace_id = %self.trace_id, "invalid trace ID (expected 32 lowercase hex characters); ignored");
            return None;
        };
        let parent = self.parent_observation_id.as_deref().and_then(|id| {
            let parsed = parse_hex_id(id, 16)
                .and_then(|hex| SpanId::from_hex(hex).ok())
                .filter(|id| *id != SpanId::INVALID);
            if parsed.is_none() {
                tracing::warn!(
                    parent_observation_id = id,
                    "invalid observation ID (expected 16 lowercase hex characters); ignored"
                );
            }
            parsed
        });
        let parent = parent.unwrap_or_else(|| {
            let id: [u8; 8] = rand::random::<u64>().max(1).to_be_bytes();
            SpanId::from_bytes(id)
        });
        Some(SpanContext::new(
            trace_id,
            parent,
            TraceFlags::SAMPLED,
            false,
            TraceState::default(),
        ))
    }
}

impl TraceContext {
    /// The current context with this trace as the parent of new spans: the
    /// first observation started in it joins the trace (and is marked as its
    /// root in Langfuse, like with
    /// [`LangfuseTracer::start_observation_in_trace`](super::LangfuseTracer::start_observation_in_trace)).
    /// An invalid trace ID leaves the current context unchanged.
    pub(crate) fn current_context(&self) -> Context {
        let current = Context::current();
        match self.span_context() {
            Some(remote) => {
                let parent = remote.span_id();
                current
                    .with_remote_span_context(remote)
                    .with_value(RemoteTraceParent(parent))
            }
            None => current,
        }
    }

    /// Make this trace the parent of the spans started until the guard is
    /// dropped, for synchronous code; see [`in_trace`].
    ///
    /// The guard must be dropped on the thread that created it, in reverse
    /// order of creation (it is not `Send`).
    pub fn attach(&self) -> opentelemetry::ContextGuard {
        self.current_context().attach()
    }
}

/// Run `future` with `trace` as the parent of the observations started in it:
/// the first one joins the existing trace, under
/// [`parent_observation_id`](TraceContext::parent_observation_id) if set,
/// and the ones started inside it are its children as usual.
///
/// This is how a function annotated with `#[observe]` (feature `macros`) is
/// attached to a trace from elsewhere, the equivalent of the
/// `langfuse_trace_id` and `langfuse_parent_observation_id` arguments of the
/// Python decorator. [`TraceContext::attach`] does the same for synchronous
/// code. An invalid trace ID is ignored with a warning.
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::{LangfuseTracer, ObservationType, TraceContext, in_trace};
///
/// # async fn example(tracer: &LangfuseTracer, trace_id: String) {
/// let trace = TraceContext::new(trace_id);
/// in_trace(trace, async {
///     let span = tracer.start_observation("follow-up", ObservationType::Span);
///     span.end();
/// })
/// .await;
/// # }
/// ```
pub async fn in_trace<F: Future>(trace: TraceContext, future: F) -> F::Output {
    // Built when first polled, like the other scopes.
    let cx = trace.current_context();
    future.with_context(cx).await
}

/// Marks the remote parent set by [`in_trace`]: an observation started
/// directly under it is the root of the trace in Langfuse.
#[derive(Debug, Clone, Copy)]
struct RemoteTraceParent(SpanId);

fn parse_hex_id(id: &str, len: usize) -> Option<&str> {
    (id.len() == len && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))).then_some(id)
}

/// The observation made current by [`Observation::in_scope`], stored in the
/// OpenTelemetry context.
#[derive(Clone)]
pub(crate) struct CurrentObservation(pub(crate) Observation);

/// A Langfuse observation (span, generation, event, ...).
///
/// Clones refer to the same observation. Setters record data until the
/// observation ends: at [`end`](Self::end), or when the last clone (and every
/// context made from it) is dropped. They never fail: data that cannot be
/// serialized is skipped with a warning. Calls after the end are ignored.
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::{Level, LangfuseTracer, ObservationType};
/// use serde_json::json;
///
/// # fn example(tracer: &LangfuseTracer) {
/// let span = tracer.start_observation("handle-request", ObservationType::Span);
/// span.set_input(&json!({"question": "Where is my order?"}));
///
/// let generation = span.start_child("answer", ObservationType::Generation);
/// generation
///     .set_model("gpt-4o")
///     .set_usage_details(&json!({"input": 120, "output": 45}))
///     .set_output("It ships tomorrow.");
/// generation.end();
///
/// span.set_level(Level::Warning).set_status_message("served from cache");
/// span.end();
/// # }
/// ```
#[derive(Clone)]
pub struct Observation {
    inner: Arc<Inner>,
}

struct Inner {
    /// Parent context plus this span.
    cx: Context,
    kind: ObservationType,
    span_id: SpanId,
    trace_id: String,
    id: String,
    start_time: SystemTime,
    recording: bool,
    /// Effective environment: propagated, else the tracer default.
    environment: Option<String>,
    shared: Arc<TracerShared>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    attributes: BTreeMap<String, OtelValue>,
    level: Option<Level>,
    status_message: Option<String>,
    ended: bool,
}

impl fmt::Debug for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Observation")
            .field("kind", &self.inner.kind)
            .field("trace_id", &self.inner.trace_id)
            .field("id", &self.inner.id)
            .finish_non_exhaustive()
    }
}

impl Observation {
    /// Start an observation under `parent_cx`.
    pub(crate) fn start(
        shared: &Arc<TracerShared>,
        name: String,
        kind: ObservationType,
        parent_cx: &Context,
        as_root: bool,
    ) -> Self {
        let start_time = SystemTime::now();
        let mut initial = vec![KeyValue::new(attr::OBSERVATION_TYPE, kind.as_str())];
        let as_root = as_root
            || parent_cx
                .get::<RemoteTraceParent>()
                .is_some_and(|remote| remote.0 == parent_cx.span().span_context().span_id());
        if as_root {
            initial.push(KeyValue::new(attr::AS_ROOT, true));
        }
        let builder = shared
            .tracer
            .span_builder(name)
            .with_start_time(start_time)
            .with_attributes(initial.clone());
        let span = shared.tracer.build_with_context(builder, parent_cx);

        let recording = span.is_recording();
        let span_context = span.span_context().clone();
        // Attributes the processor propagated in `on_start`.
        let propagated_environment = span.exported_data().and_then(|data| {
            attr::string_attribute(&data.attributes, attr::ENVIRONMENT).map(str::to_owned)
        });

        let mut state = State::default();
        for kv in initial {
            state.attributes.insert(kv.key.to_string(), kv.value);
        }
        if propagated_environment.is_none()
            && let Some(environment) = &shared.environment
        {
            state
                .attributes
                .insert(attr::ENVIRONMENT.into(), environment.clone().into());
        }
        if let Some(release) = &shared.release {
            state
                .attributes
                .insert(attr::RELEASE.into(), release.clone().into());
        }

        let environment = propagated_environment.or_else(|| shared.environment.clone());
        Self {
            inner: Arc::new(Inner {
                cx: parent_cx.with_span(span),
                kind,
                span_id: span_context.span_id(),
                trace_id: span_context.trace_id().to_string(),
                id: span_context.span_id().to_string(),
                start_time,
                recording,
                environment,
                shared: Arc::clone(shared),
                state: Mutex::new(state),
            }),
        }
    }

    /// The observation made current by [`in_scope`](Self::in_scope), if any.
    pub fn current() -> Option<Observation> {
        Context::current()
            .get::<CurrentObservation>()
            .map(|c| c.0.clone())
    }

    /// The trace ID (32 lowercase hex characters).
    pub fn trace_id(&self) -> &str {
        &self.inner.trace_id
    }

    /// The observation ID (16 lowercase hex characters).
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// The observation type.
    pub fn kind(&self) -> ObservationType {
        self.inner.kind
    }

    /// Whether the observation is recorded (`false` when sampled out or with
    /// tracing disabled; setters are then no-ops).
    pub fn is_recording(&self) -> bool {
        self.inner.recording
    }

    /// The OpenTelemetry context in which this observation is current: pass it
    /// to other instrumentation or to a propagator to continue the trace.
    pub fn context(&self) -> Context {
        overlay_current(self.inner.cx.clone())
            .with_value(CurrentObservation(self.clone()))
            .with_baggage([KeyValue::new(
                TRACE_ID_BAGGAGE_KEY,
                self.inner.trace_id.clone(),
            )])
    }

    /// Run `future` with this observation as the current one: observations
    /// started inside (also by other instrumentation on the same provider)
    /// become its children, and [`Observation::current`] returns it.
    ///
    /// The context follows the future, not tasks spawned from it: wrap
    /// spawned futures with `in_scope` too.
    pub fn in_scope<F: Future>(&self, future: F) -> impl Future<Output = F::Output> {
        let observation = self.clone();
        // Built when first polled, so outer scopes (e.g. `propagate_attributes`
        // wrapping this future) are taken into account.
        async move { future.with_context(observation.context()).await }
    }

    /// Start a child observation.
    pub fn start_child(&self, name: impl Into<String>, kind: ObservationType) -> Observation {
        let current = Context::current();
        let parent_cx = if current.span().span_context().span_id() == self.inner.span_id {
            current
        } else {
            overlay_current(self.inner.cx.clone()).with_value(CurrentObservation(self.clone()))
        };
        Observation::start(&self.inner.shared, name.into(), kind, &parent_cx, false)
    }

    /// Rename the observation.
    pub fn set_name(&self, name: impl Into<String>) -> &Self {
        if self.inner.recording {
            self.inner.cx.span().update_name(name.into());
        }
        self
    }

    /// Record the input: strings as-is, anything else as JSON.
    pub fn set_input<T: Serialize + ?Sized>(&self, input: &T) -> &Self {
        self.set_io(attr::OBSERVATION_INPUT, input)
    }

    /// Record the output: strings as-is, anything else as JSON.
    pub fn set_output<T: Serialize + ?Sized>(&self, output: &T) -> &Self {
        self.set_io(attr::OBSERVATION_OUTPUT, output)
    }

    /// Record metadata. An object is merged key by key into earlier metadata
    /// (`null` values are skipped); any other value replaces it.
    pub fn set_metadata<T: Serialize + ?Sized>(&self, metadata: &T) -> &Self {
        if let Some(value) = self.masked(metadata, "metadata") {
            let entries = flatten_metadata(attr::OBSERVATION_METADATA, value);
            self.with_state(|state| {
                for (key, value) in entries {
                    state.attributes.insert(key, value.into());
                }
            });
        }
        self
    }

    /// Record the version of the code or component.
    pub fn set_version(&self, version: impl Into<String>) -> &Self {
        self.set_attribute(attr::VERSION, version.into().into())
    }

    /// Record the level; [`Level::Error`] also marks the span as failed.
    pub fn set_level(&self, level: Level) -> &Self {
        self.with_state(|state| {
            state.level = Some(level);
            state
                .attributes
                .insert(attr::OBSERVATION_LEVEL.into(), level.as_str().into());
        });
        self
    }

    /// Record a status message (e.g. the error of a failed step).
    pub fn set_status_message(&self, message: impl Into<String>) -> &Self {
        let message = message.into();
        self.with_state(|state| {
            state.attributes.insert(
                attr::OBSERVATION_STATUS_MESSAGE.into(),
                message.clone().into(),
            );
            state.status_message = Some(message);
        });
        self
    }

    /// Record a failure: level [`Level::Error`], `error` (its `Display`, or its
    /// type name when that is empty) as status message, and an OpenTelemetry
    /// `exception` event with its type and message. This is what
    /// `#[observe]` records for an `Err` result.
    pub fn record_error<E: fmt::Display + ?Sized>(&self, error: &E) -> &Self {
        self.record_error_parts(std::any::type_name::<E>(), error.to_string())
    }

    pub(crate) fn record_error_parts(&self, type_name: &str, message: String) -> &Self {
        if !self.inner.recording || self.is_ended() {
            return self;
        }
        let message = if message.is_empty() {
            type_name.to_owned()
        } else {
            message
        };
        self.set_level(Level::Error)
            .set_status_message(message.clone());
        self.inner.cx.span().add_event(
            "exception",
            vec![
                KeyValue::new("exception.type", type_name.to_owned()),
                KeyValue::new("exception.message", message),
            ],
        );
        self
    }

    /// The tracer state of this observation.
    #[cfg_attr(not(feature = "macros"), allow(dead_code))]
    pub(crate) fn shared(&self) -> &Arc<TracerShared> {
        &self.inner.shared
    }

    /// Make the whole trace viewable by anyone with its link. Cannot be undone
    /// from the SDK.
    pub fn set_trace_as_public(&self) -> &Self {
        self.set_attribute(attr::TRACE_PUBLIC, true.into())
    }

    /// Record the model name (generations and embeddings only).
    pub fn set_model(&self, model: impl Into<String>) -> &Self {
        if self.generation_only("model") {
            self.set_attribute(attr::OBSERVATION_MODEL, model.into().into());
        }
        self
    }

    /// Record the model parameters, e.g. `{"temperature": 0.2}`
    /// (generations and embeddings only).
    pub fn set_model_parameters<T: Serialize + ?Sized>(&self, parameters: &T) -> &Self {
        self.set_generation_json(
            attr::OBSERVATION_MODEL_PARAMETERS,
            parameters,
            "model_parameters",
        )
    }

    /// Record token usage, e.g. `{"input": 120, "output": 45}` (generations
    /// and embeddings only).
    pub fn set_usage_details<T: Serialize + ?Sized>(&self, usage: &T) -> &Self {
        self.set_generation_json(attr::OBSERVATION_USAGE_DETAILS, usage, "usage_details")
    }

    /// Record costs, e.g. `{"input": 0.001, "output": 0.002}` (generations and
    /// embeddings only).
    pub fn set_cost_details<T: Serialize + ?Sized>(&self, cost: &T) -> &Self {
        self.set_generation_json(attr::OBSERVATION_COST_DETAILS, cost, "cost_details")
    }

    /// Record when the model started returning output, for time-to-first-token
    /// (generations and embeddings only).
    pub fn set_completion_start_time(&self, time: impl Into<DateTime<Utc>>) -> &Self {
        if self.generation_only("completion_start_time") {
            let iso = time.into().to_rfc3339_opts(SecondsFormat::Micros, true);
            self.set_attribute(
                attr::OBSERVATION_COMPLETION_START_TIME,
                serde_json::Value::String(iso).to_string().into(),
            );
        }
        self
    }

    /// Link the Langfuse prompt used (generations and embeddings only).
    /// Fallback prompts are not linked.
    pub fn set_prompt(&self, prompt: &Prompt) -> &Self {
        if self.generation_only("prompt")
            && let Some(link) = PromptLink::from_prompt(prompt)
        {
            self.with_state(|state| {
                state
                    .attributes
                    .insert(attr::OBSERVATION_PROMPT_NAME.into(), link.name.into());
                state
                    .attributes
                    .insert(attr::OBSERVATION_PROMPT_VERSION.into(), link.version.into());
            });
        }
        self
    }

    /// Score this observation: queues `score` on the client with this trace
    /// and observation and, unless set, the observation's environment (see
    /// [`LangfuseClient::score`](crate::LangfuseClient::score)). Dropped when
    /// the observation is not recorded (sampled out or tracing disabled).
    ///
    /// ```rust,no_run
    /// # use langfuse_rust_sdk::otel::Observation;
    /// use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
    /// # fn example(generation: &Observation) {
    /// generation.score(CreateScoreRequest {
    ///     comment: Some("Matches the reference answer".into()),
    ///     ..CreateScoreRequest::new("correctness", CreateScoreValue::Numeric(0.9))
    /// });
    /// # }
    /// ```
    pub fn score(&self, mut score: CreateScoreRequest) {
        score.observation_id = Some(self.inner.id.clone());
        self.queue_score(score);
    }

    /// Score the whole trace of this observation; see [`score`](Self::score).
    pub fn score_trace(&self, mut score: CreateScoreRequest) {
        score.observation_id = None;
        self.queue_score(score);
    }

    fn queue_score(&self, mut score: CreateScoreRequest) {
        if !self.inner.recording {
            tracing::debug!(score = %score.name, "observation not recorded; score dropped");
            return;
        }
        score.trace_id = Some(self.inner.trace_id.clone());
        if score.environment.is_none() {
            score.environment = self.inner.environment.clone();
        }
        self.inner.shared.client.score(score);
    }

    /// End the observation now. Later calls do nothing.
    pub fn end(&self) {
        self.inner.finish();
    }

    /// Whether the observation has ended.
    pub fn is_ended(&self) -> bool {
        self.inner.state().ended
    }

    /// Add `attributes` to the buffer (used for propagated attributes).
    pub(crate) fn buffer(&self, attributes: Vec<KeyValue>) {
        self.with_state(|state| {
            for kv in attributes {
                state.attributes.insert(kv.key.to_string(), kv.value);
            }
        });
    }

    fn set_attribute(&self, key: &str, value: OtelValue) -> &Self {
        self.with_state(|state| {
            state.attributes.insert(key.to_owned(), value);
        });
        self
    }

    fn set_io<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> &Self {
        if let Some(value) = self.masked(value, key) {
            self.set_attribute(key, io_value(value).into());
        }
        self
    }

    fn set_generation_json<T: Serialize + ?Sized>(
        &self,
        key: &str,
        value: &T,
        what: &str,
    ) -> &Self {
        if self.generation_only(what)
            && self.inner.recording
            && let Some(value) = to_json(value, what)
        {
            self.set_attribute(key, js_safe(value).to_string().into());
        }
        self
    }

    fn generation_only(&self, what: &str) -> bool {
        let applies = self.inner.kind.is_generation_like();
        if !applies {
            tracing::debug!(
                field = what,
                kind = self.inner.kind.as_str(),
                "only generations and embeddings record this field; ignored"
            );
        }
        applies
    }

    /// Serialize and mask `value`; `None` when not recording or not serializable.
    fn masked<T: Serialize + ?Sized>(&self, value: &T, what: &str) -> Option<serde_json::Value> {
        if !self.inner.recording {
            return None;
        }
        let value = to_json(value, what)?;
        match &self.inner.shared.mask {
            None => Some(value),
            Some(mask) => match catch_unwind(AssertUnwindSafe(|| mask(value))) {
                Ok(masked) => Some(masked),
                Err(_) => {
                    tracing::error!(field = what, "mask function panicked; value fully masked");
                    Some(serde_json::Value::String(MASK_FALLBACK.into()))
                }
            },
        }
    }

    fn with_state(&self, f: impl FnOnce(&mut State)) {
        if !self.inner.recording {
            return;
        }
        let mut state = self.inner.state();
        if state.ended {
            tracing::debug!(observation = %self.inner.id, "observation already ended; update ignored");
            return;
        }
        f(&mut state);
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn finish(&self) {
        let (attributes, status) = {
            let mut state = self.state();
            if state.ended {
                return;
            }
            state.ended = true;
            let attributes: Vec<KeyValue> = std::mem::take(&mut state.attributes)
                .into_iter()
                .map(|(k, v)| KeyValue::new(k, v))
                .collect();
            let status = (state.level == Some(Level::Error))
                .then(|| Status::error(state.status_message.clone().unwrap_or_default()));
            (attributes, status)
        };
        let span = self.cx.span();
        if self.recording {
            span.set_attributes(attributes);
            if let Some(status) = status {
                span.set_status(status);
            }
        }
        if self.kind == ObservationType::Event {
            span.end_with_timestamp(self.start_time);
        } else {
            span.end();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Copy the propagated attributes and baggage of the current context onto `cx`.
fn overlay_current(mut cx: Context) -> Context {
    let current = Context::current();
    if let Some(state) = current.get::<PropagatedState>() {
        cx = cx.with_value(state.clone());
    }
    let baggage: Vec<KeyValue> = current
        .baggage()
        .iter()
        .map(|(key, (value, _))| KeyValue::new(key.clone(), value.clone()))
        .collect();
    if !baggage.is_empty() {
        cx = cx.with_baggage(baggage);
    }
    cx
}

/// Apply `attributes` to the current observation's buffer when `span` is that
/// observation, or directly to `span` otherwise.
pub(crate) fn set_or_buffer(
    current: Option<&Observation>,
    span: &SpanRef<'_>,
    attributes: Vec<KeyValue>,
) {
    if attributes.is_empty() {
        return;
    }
    if let Some(observation) = current
        && observation.inner.span_id == span.span_context().span_id()
    {
        observation.buffer(attributes);
        return;
    }
    if span.is_recording() {
        span.set_attributes(attributes);
    }
}

/// Trace ID of the current span (Langfuse observation or other), if any.
pub fn current_trace_id() -> Option<String> {
    let cx = Context::current();
    let span_context = cx.span().span_context().clone();
    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

/// ID of the current span (Langfuse observation or other), if any.
pub fn current_observation_id() -> Option<String> {
    let cx = Context::current();
    let span_context = cx.span().span_context().clone();
    span_context
        .is_valid()
        .then(|| span_context.span_id().to_string())
}
