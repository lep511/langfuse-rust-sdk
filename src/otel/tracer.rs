//! The Langfuse tracer: OpenTelemetry provider, processor and export task.

use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use opentelemetry::trace::{SpanId, SpanKind, TraceContextExt, TraceId, TracerProvider as _};
use opentelemetry::{Context, InstrumentationScope, KeyValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{
    IdGenerator, Sampler, SamplingDecision, SdkTracer, SdkTracerProvider, ShouldSample,
};
use tokio::sync::oneshot;

use super::attributes as attr;
use super::exporter::Command;
use super::observation::{Observation, ObservationType, TraceContext};
use super::options::{MaskFn, TracingOptions};
use super::processor::{LANGFUSE_TRACER_NAME, LangfuseSpanProcessor};
use crate::LangfuseClient;
use crate::error::LangfuseError;
use crate::models::scores::CreateScoreRequest;

/// The tracer of [`LangfuseTracer::install_global`].
static GLOBAL_TRACER: RwLock<Option<LangfuseTracer>> = RwLock::new(None);

/// State shared by the tracer and its observations.
pub(crate) struct TracerShared {
    pub(crate) tracer: SdkTracer,
    /// Receives the scores of observations.
    pub(crate) client: LangfuseClient,
    pub(crate) environment: Option<String>,
    pub(crate) release: Option<String>,
    pub(crate) mask: Option<MaskFn>,
    /// Share of traces recorded, applied to [`LangfuseTracer::score`].
    pub(crate) sample_rate: f64,
    /// Attributes a span can hold (`OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT`, default
    /// 128); observations keep their core attributes within it.
    pub(crate) max_attributes: usize,
}

/// The span attribute limit of a provider built with the default
/// configuration: `OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT` when it is a valid `u32`
/// (as `opentelemetry_sdk` reads it), else 128.
fn span_attribute_limit() -> usize {
    std::env::var("OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT")
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(DEFAULT_MAX_ATTRIBUTES_PER_SPAN) as usize
}

/// `opentelemetry_sdk`'s default `SpanLimits::max_attributes_per_span`.
const DEFAULT_MAX_ATTRIBUTES_PER_SPAN: u32 = 128;

/// Creates Langfuse observations and exports them in the background.
///
/// Build it with [`LangfuseClient::tracer`] (or [`LangfuseTracer::new`])
/// inside a Tokio runtime. Clones share the same provider and export task.
/// Call [`shutdown`](Self::shutdown) before the program exits so queued spans
/// are sent.
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::{ObservationType, TracingOptions};
/// use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
///
/// # async fn example() -> Result<(), langfuse_rust_sdk::LangfuseError> {
/// let client = LangfuseClient::new(ClientConfig::from_env()?)?;
/// let tracer = client.tracer(TracingOptions::from_env()?)?;
///
/// let span = tracer.start_observation("handle-request", ObservationType::Span);
/// span.in_scope(async {
///     // Observations started here are children of `span`.
/// })
/// .await;
/// span.end();
///
/// tracer.shutdown().await;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct LangfuseTracer {
    shared: Arc<TracerShared>,
    provider: SdkTracerProvider,
    processor: LangfuseSpanProcessor,
    /// Whether the tracer built `provider` (and shuts it down).
    owns_provider: bool,
}

impl fmt::Debug for LangfuseTracer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseTracer")
            .field("enabled", &self.is_enabled())
            .finish_non_exhaustive()
    }
}

/// Generator of [`TracingOptions::id_generator`], shared with the options.
#[derive(Debug)]
struct SharedIdGenerator(Arc<dyn IdGenerator>);

impl IdGenerator for SharedIdGenerator {
    fn new_trace_id(&self) -> TraceId {
        self.0.new_trace_id()
    }

    fn new_span_id(&self) -> SpanId {
        self.0.new_span_id()
    }
}

impl LangfuseTracer {
    /// Create a tracer exporting through `client` (see [`LangfuseClient::tracer`]),
    /// on a tracer provider of its own.
    ///
    /// Returns [`LangfuseError::Config`] for invalid options or when called
    /// outside a Tokio runtime with tracing enabled.
    pub fn new(client: &LangfuseClient, options: TracingOptions) -> Result<Self, LangfuseError> {
        let processor = LangfuseSpanProcessor::new(client, options)?;
        let provider = Self::own_provider(&processor);
        Ok(Self::assemble(processor, provider, true))
    }

    /// Create a tracer on `provider`, a tracer provider you built with
    /// `processor` among its span processors (see [`LangfuseSpanProcessor`]),
    /// so the same provider also feeds other backends. Observations are
    /// created with `provider`'s sampler, ID generator, span limits and
    /// resource; [`TracingOptions::id_generator`] is ignored (with a warning),
    /// and [`TracingOptions::sample_rate`] only applies to
    /// [`score`](Self::score): give `provider` the matching
    /// `Sampler::TraceIdRatioBased`. Environment and release are still
    /// recorded on every observation.
    ///
    /// Observations keep their core attributes (output, model, usage, ...)
    /// within the span attribute limit, dropping metadata first; they assume
    /// the default limit (`OTEL_SPAN_ATTRIBUTE_COUNT_LIMIT`, else 128), so a
    /// provider built with other `SpanLimits` should keep that limit or a
    /// higher one.
    ///
    /// [`shutdown`](Self::shutdown) stops the Langfuse export but leaves
    /// `provider` running; shut it down yourself. With tracing disabled,
    /// `provider` is not used and the tracer records nothing.
    ///
    /// Returns [`LangfuseError::Config`] when `provider` was not built with
    /// `processor` (no provider has given it its resource).
    pub fn with_provider(
        processor: &LangfuseSpanProcessor,
        provider: SdkTracerProvider,
    ) -> Result<Self, LangfuseError> {
        if !processor.is_enabled() {
            let provider = Self::own_provider(processor);
            return Ok(Self::assemble(processor.clone(), provider, true));
        }
        if !processor.is_registered() {
            return Err(LangfuseError::Config(
                "the tracer provider does not include the LangfuseSpanProcessor: add a clone of it with `with_span_processor` before `build`".into(),
            ));
        }
        if processor.settings().id_generator.is_some() {
            tracing::warn!(
                "TracingOptions::id_generator is ignored with LangfuseTracer::with_provider; configure the ID generator on the tracer provider instead"
            );
        }
        Ok(Self::assemble(processor.clone(), provider, false))
    }

    /// The provider of [`new`](Self::new): resource with environment and
    /// release, sampler and ID generator from the options.
    fn own_provider(processor: &LangfuseSpanProcessor) -> SdkTracerProvider {
        let settings = processor.settings();
        let mut resource = Resource::builder();
        if let Some(environment) = &settings.environment {
            resource =
                resource.with_attribute(KeyValue::new(attr::ENVIRONMENT, environment.clone()));
        }
        if let Some(release) = &settings.release {
            resource = resource.with_attribute(KeyValue::new(attr::RELEASE, release.clone()));
        }
        let mut builder = SdkTracerProvider::builder().with_resource(resource.build());
        if let Some(id_generator) = &settings.id_generator {
            builder = builder.with_id_generator(SharedIdGenerator(Arc::clone(id_generator)));
        }
        if processor.is_enabled() {
            builder = builder.with_span_processor(processor.clone());
            if settings.sample_rate < 1.0 {
                builder = builder.with_sampler(Sampler::TraceIdRatioBased(settings.sample_rate));
            }
        } else {
            builder = builder.with_sampler(Sampler::AlwaysOff);
        }
        builder.build()
    }

    fn assemble(
        processor: LangfuseSpanProcessor,
        provider: SdkTracerProvider,
        owns_provider: bool,
    ) -> Self {
        let settings = processor.settings();
        let scope = InstrumentationScope::builder(LANGFUSE_TRACER_NAME)
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_attributes([KeyValue::new(
                "public_key",
                processor.public_key().to_owned(),
            )])
            .build();
        let tracer = provider.tracer_with_scope(scope);
        Self {
            shared: Arc::new(TracerShared {
                tracer,
                client: settings.client.clone(),
                environment: settings.environment.clone(),
                release: settings.release.clone(),
                mask: settings.mask.clone(),
                sample_rate: settings.sample_rate,
                max_attributes: span_attribute_limit(),
            }),
            provider,
            processor,
            owns_provider,
        }
    }

    /// State shared with the observations of this tracer.
    pub(crate) fn shared(&self) -> &Arc<TracerShared> {
        &self.shared
    }

    /// Make this tracer the global one, used by `#[observe]` (feature
    /// `macros`) when no observation is current; returns the one it replaces.
    ///
    /// Inside an observation, `#[observe]` uses that observation's tracer, so
    /// the global tracer is only needed for the first (root) observations.
    /// Without either, observed functions run without tracing.
    pub fn install_global(&self) -> Option<LangfuseTracer> {
        GLOBAL_TRACER
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(self.clone())
    }

    /// The tracer of [`install_global`](Self::install_global), if any.
    pub fn global() -> Option<LangfuseTracer> {
        GLOBAL_TRACER
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Remove the global tracer and return it.
    pub fn uninstall_global() -> Option<LangfuseTracer> {
        GLOBAL_TRACER
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Whether tracing is enabled (spans are exported and scores queued).
    pub fn is_enabled(&self) -> bool {
        self.processor.is_enabled()
    }

    /// The OpenTelemetry tracer provider. Use it to get tracers for other
    /// instrumentation (their spans are exported when
    /// [`should_export_span`](TracingOptions::should_export_span) accepts them)
    /// or register it with `opentelemetry::global::set_tracer_provider`.
    pub fn provider(&self) -> &SdkTracerProvider {
        &self.provider
    }

    /// Start an observation, as a child of the current observation (or span)
    /// if there is one.
    pub fn start_observation(&self, name: impl Into<String>, kind: ObservationType) -> Observation {
        Observation::start(&self.shared, name.into(), kind, &Context::current(), false)
    }

    /// Start an observation in an existing trace, under the given parent
    /// observation if any. An invalid trace ID is ignored with a warning and
    /// the observation starts as with [`start_observation`](Self::start_observation).
    pub fn start_observation_in_trace(
        &self,
        name: impl Into<String>,
        kind: ObservationType,
        trace: &TraceContext,
    ) -> Observation {
        let current = Context::current();
        match trace.span_context() {
            Some(remote_parent) => Observation::start(
                &self.shared,
                name.into(),
                kind,
                &current.with_remote_span_context(remote_parent),
                true,
            ),
            None => Observation::start(&self.shared, name.into(), kind, &current, false),
        }
    }

    /// Queue `score` on the client like [`LangfuseClient::score`], applying
    /// this tracer's sampling: a score for a trace the tracer samples out is
    /// dropped, like the trace itself. The decision only depends on the trace
    /// ID, so it matches the spans. Scores without a W3C `trace_id` (32
    /// lowercase hex characters; e.g. session or dataset run scores, IDs from
    /// other systems) are always queued. A missing `environment` is set to
    /// the tracer's. With tracing disabled the score is dropped.
    ///
    /// Prefer this over [`LangfuseClient::score`] when
    /// [`sample_rate`](TracingOptions::sample_rate) is below `1.0`.
    pub fn score(&self, mut score: CreateScoreRequest) {
        if !self.is_enabled() {
            tracing::debug!(score = %score.name, "tracing is disabled; score dropped");
            return;
        }
        if let Some(trace_id) = score.trace_id.as_deref()
            && !self.samples(trace_id)
        {
            tracing::debug!(score = %score.name, trace_id, "trace is sampled out; score dropped");
            return;
        }
        if score.environment.is_none() {
            score.environment = self.shared.environment.clone();
        }
        self.shared.client.score(score);
    }

    /// Whether the sampler records trace `trace_id`; `true` for strings that
    /// are not W3C trace IDs.
    fn samples(&self, trace_id: &str) -> bool {
        if self.shared.sample_rate >= 1.0 {
            return true;
        }
        let is_w3c = trace_id.len() == 32
            && trace_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let id = match TraceId::from_hex(trace_id) {
            Ok(id) if is_w3c && id != TraceId::INVALID => id,
            _ => return true,
        };
        Sampler::TraceIdRatioBased(self.shared.sample_rate)
            .should_sample(None, id, "score", &SpanKind::Internal, &[], &[])
            .decision
            == SamplingDecision::RecordAndSample
    }

    /// Score the current span (Langfuse observation or other); see
    /// [`Observation::score`]. Dropped with a warning when there is no current
    /// span.
    pub fn score_current_span(&self, score: CreateScoreRequest) {
        self.score_current(score, false);
    }

    /// Score the trace of the current span; see [`Observation::score_trace`].
    pub fn score_current_trace(&self, score: CreateScoreRequest) {
        self.score_current(score, true);
    }

    fn score_current(&self, mut score: CreateScoreRequest, trace_level: bool) {
        let cx = Context::current();
        let span_context = cx.span().span_context().clone();
        if !span_context.is_valid() {
            tracing::warn!(score = %score.name, "no current span to score; score dropped");
            return;
        }
        if let Some(observation) =
            Observation::current().filter(|o| o.id() == span_context.span_id().to_string())
        {
            if trace_level {
                observation.score_trace(score);
            } else {
                observation.score(score);
            }
            return;
        }
        if !span_context.is_sampled() {
            tracing::debug!(score = %score.name, "current span is not sampled; score dropped");
            return;
        }
        score.trace_id = Some(span_context.trace_id().to_string());
        score.observation_id = (!trace_level).then(|| span_context.span_id().to_string());
        if score.environment.is_none() {
            score.environment = self.shared.environment.clone();
        }
        self.shared.client.score(score);
    }

    /// Export every finished span now, then send the client's queued scores
    /// and media uploads, and wait for all of it. Failures are logged through
    /// `tracing`, not returned.
    ///
    /// The client's queues are included on purpose, in this order: exporting
    /// a span queues the uploads of the media found in it, so a flush that
    /// stopped at the spans would leave references to files not uploaded yet.
    /// It also waits for scores queued elsewhere on the same client.
    pub async fn flush(&self) {
        self.send_and_wait(Command::Flush).await;
        self.shared.client.flush().await;
    }

    /// Export every finished span, then stop exporting (later spans are
    /// dropped), and flush the client's scores and media uploads (see
    /// [`flush`](Self::flush) for why). The client itself keeps working; call
    /// [`LangfuseClient::shutdown`] to stop its background tasks too.
    pub async fn shutdown(&self) {
        self.send_and_wait(Command::Shutdown).await;
        if self.owns_provider {
            let _ = self.provider.shutdown();
        }
        self.shared.client.flush().await;
    }

    async fn send_and_wait(&self, command: fn(Option<oneshot::Sender<()>>) -> Command) {
        let Some(commands) = self.processor.commands() else {
            return;
        };
        let (ack, done) = oneshot::channel();
        if commands.send(command(Some(ack))).await.is_ok() {
            let _ = done.await;
        }
    }
}
