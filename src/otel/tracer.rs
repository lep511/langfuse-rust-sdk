//! The Langfuse tracer: OpenTelemetry provider, processor and export task.

use std::fmt;
use std::sync::{Arc, OnceLock};

use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::{Context, InstrumentationScope, KeyValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracer, SdkTracerProvider};
use tokio::sync::{mpsc, oneshot};

use super::attributes as attr;
use super::exporter::{Command, ExportTask};
use super::observation::{Observation, ObservationType, TraceContext};
use super::options::{MaskFn, TracingOptions};
use super::processor::{LANGFUSE_TRACER_NAME, LangfuseSpanProcessor};
use crate::LangfuseClient;
use crate::error::LangfuseError;

/// State shared by the tracer and its observations.
pub(crate) struct TracerShared {
    pub(crate) tracer: SdkTracer,
    pub(crate) environment: Option<String>,
    pub(crate) release: Option<String>,
    pub(crate) mask: Option<MaskFn>,
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
                ingestion: client.ingestion.clone(),
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
                environment,
                release,
                mask,
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

    /// Export every finished span now and wait for it. Export failures are
    /// logged through `tracing`, not returned.
    pub async fn flush(&self) {
        self.send_and_wait(Command::Flush).await;
    }

    /// Export every finished span, then stop exporting; later spans are
    /// dropped. Waits for the export.
    pub async fn shutdown(&self) {
        self.send_and_wait(Command::Shutdown).await;
        let _ = self.provider.shutdown();
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
