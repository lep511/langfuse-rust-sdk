//! The Langfuse tracer: OpenTelemetry provider, processor and export task.

use std::fmt;
use std::sync::{Arc, OnceLock};

use opentelemetry::trace::{SpanKind, TraceContextExt, TraceId, TracerProvider as _};
use opentelemetry::{Context, InstrumentationScope, KeyValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{
    Sampler, SamplingDecision, SdkTracer, SdkTracerProvider, ShouldSample,
};
use tokio::sync::{mpsc, oneshot};

use super::attributes as attr;
use super::exporter::{Command, ExportTask};
use super::observation::{Observation, ObservationType, TraceContext};
use super::options::{MaskFn, TracingOptions};
use super::processor::{LANGFUSE_TRACER_NAME, LangfuseSpanProcessor};
use crate::LangfuseClient;
use crate::error::LangfuseError;
use crate::models::scores::CreateScoreRequest;

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
}

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
    commands: Option<mpsc::Sender<Command>>,
}

impl fmt::Debug for LangfuseTracer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseTracer")
            .field("enabled", &self.commands.is_some())
            .finish_non_exhaustive()
    }
}

impl LangfuseTracer {
    /// Create a tracer exporting through `client` (see [`LangfuseClient::tracer`]).
    ///
    /// Returns [`LangfuseError::Config`] for invalid options or when called
    /// outside a Tokio runtime with tracing enabled.
    pub fn new(client: &LangfuseClient, options: TracingOptions) -> Result<Self, LangfuseError> {
        let TracingOptions {
            environment,
            release,
            sample_rate,
            enabled,
            flush_at,
            flush_interval,
            max_queue_size,
            max_batch_bytes,
            mask,
            should_export_span,
            media_upload,
        } = options;
        let invalid = |msg: &str| {
            Err(LangfuseError::Config(format!(
                "Invalid tracing options: {msg}"
            )))
        };
        if !(0.0..=1.0).contains(&sample_rate) {
            return invalid("sample_rate must be between 0 and 1");
        }
        if flush_at == 0 || max_queue_size == 0 || max_batch_bytes == 0 || flush_interval.is_zero()
        {
            return invalid(
                "flush_at, max_queue_size, max_batch_bytes and flush_interval must be greater than 0",
            );
        }

        let public_key = client.public_key().to_owned();
        let mut resource = Resource::builder();
        if let Some(environment) = &environment {
            resource =
                resource.with_attribute(KeyValue::new(attr::ENVIRONMENT, environment.clone()));
        }
        if let Some(release) = &release {
            resource = resource.with_attribute(KeyValue::new(attr::RELEASE, release.clone()));
        }
        let mut builder = SdkTracerProvider::builder().with_resource(resource.build());

        let commands = if enabled {
            let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
                LangfuseError::Config(
                    "LangfuseTracer must be created inside a Tokio runtime".into(),
                )
            })?;
            let (sender, receiver) = mpsc::channel(max_queue_size);
            let resource_cell = Arc::new(OnceLock::new());
            builder = builder.with_span_processor(LangfuseSpanProcessor::new(
                public_key.clone(),
                should_export_span,
                sender.clone(),
                Arc::clone(&resource_cell),
            ));
            if sample_rate < 1.0 {
                builder = builder.with_sampler(Sampler::TraceIdRatioBased(sample_rate));
            }
            let task = ExportTask {
                client: client.clone(),
                media_upload,
                resource: resource_cell,
                flush_at,
                flush_interval,
                max_batch_bytes,
            };
            runtime.spawn(task.run(receiver));
            Some(sender)
        } else {
            builder = builder.with_sampler(Sampler::AlwaysOff);
            None
        };

        let provider = builder.build();
        let scope = InstrumentationScope::builder(LANGFUSE_TRACER_NAME)
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_attributes([KeyValue::new("public_key", public_key)])
            .build();
        let tracer = provider.tracer_with_scope(scope);

        Ok(Self {
            shared: Arc::new(TracerShared {
                tracer,
                client: client.clone(),
                environment,
                release,
                mask,
                sample_rate,
            }),
            provider,
            commands,
        })
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
        if self.commands.is_none() {
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
        let _ = self.provider.shutdown();
        self.shared.client.flush().await;
    }

    async fn send_and_wait(&self, command: fn(Option<oneshot::Sender<()>>) -> Command) {
        let Some(commands) = &self.commands else {
            return;
        };
        let (ack, done) = oneshot::channel();
        if commands.send(command(Some(ack))).await.is_ok() {
            let _ = done.await;
        }
    }
}
