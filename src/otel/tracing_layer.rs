//! Bridge from the `tracing` crate (feature `tracing-layer`).

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{SdkTracer, SpanData};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;

use super::tracer::LangfuseTracer;

/// Instrumentation scope of the spans recorded by [`tracing_layer`].
pub const TRACING_LAYER_SCOPE: &str = "tracing";

/// A `tracing-opentelemetry` layer that records `tracing` spans on the
/// tracer's provider, so they join Langfuse traces (feature `tracing-layer`).
///
/// Libraries that describe LLM calls with `gen_ai.*` span fields, like
/// [rig](https://docs.rs/rig-core) (`rig::completions` spans with model,
/// token usage and, when the request enables `record_content_telemetry`,
/// input and output messages), show up as generations. Context flows both
/// ways: `tracing` spans started inside
/// [`Observation::in_scope`](super::Observation::in_scope) are children of the
/// observation, and observations started while a `tracing` span is entered
/// are children of that span.
///
/// The default export filter only keeps `gen_ai.*` spans from this layer;
/// accept the others with [`is_tracing_layer_span`] (and filter the layer by
/// target, or every span of every library is exported):
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::{self, TracingOptions};
/// use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
/// use tracing_subscriber::filter::{LevelFilter, Targets};
/// use tracing_subscriber::prelude::*;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = LangfuseClient::new(ClientConfig::from_env()?)?;
/// let options = TracingOptions::from_env()?.with_should_export_span(|span| {
///     otel::is_default_export_span(span) || otel::is_tracing_layer_span(span)
/// });
/// let tracer = client.tracer(options)?;
///
/// let targets = Targets::new()
///     .with_target("rig", LevelFilter::INFO)
///     .with_target("my_app", LevelFilter::INFO);
/// tracing_subscriber::registry()
///     .with(otel::tracing_layer(&tracer).with_filter(targets))
///     .init();
///
/// // ... `#[tracing::instrument]` functions and rig calls are now traced.
/// tracer.shutdown().await;
/// # Ok(())
/// # }
/// ```
///
/// Busy and idle times (`busy_ns`, `idle_ns`) are not recorded; the returned
/// layer has the other `tracing-opentelemetry` defaults (source location,
/// thread, target and level attributes, which Langfuse shows as metadata) and
/// its builder methods to change them. `tracing-opentelemetry` 0.34 is the
/// version built on the same `opentelemetry` 0.33 as this crate.
pub fn tracing_layer<S>(tracer: &LangfuseTracer) -> OpenTelemetryLayer<S, SdkTracer>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    tracing_opentelemetry::layer()
        .with_tracer(tracer.provider().tracer(TRACING_LAYER_SCOPE))
        .with_tracked_inactivity(false)
}

/// Whether the span was recorded by [`tracing_layer`].
pub fn is_tracing_layer_span(span: &SpanData) -> bool {
    span.instrumentation_scope.name() == TRACING_LAYER_SCOPE
}
