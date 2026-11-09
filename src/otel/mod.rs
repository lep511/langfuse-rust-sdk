//! Native tracing on OpenTelemetry (feature `otel`).
//!
//! A [`LangfuseTracer`] creates [`Observation`]s (spans, generations, agents,
//! tools, ...) as OpenTelemetry spans with the attributes Langfuse expects,
//! and exports them in batches to `POST /api/public/otel/v1/traces` through
//! the client's HTTP stack (authentication, retries, custom `reqwest` client).
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::otel::{ObservationType, PropagatedAttributes, TracingOptions, propagate_attributes};
//! use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
//! use serde_json::json;
//!
//! # async fn call_llm(_: &str) -> String { String::new() }
//! # async fn example() -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let client = LangfuseClient::new(ClientConfig::from_env()?)?;
//! let tracer = client.tracer(TracingOptions::from_env()?)?;
//!
//! let request = tracer.start_observation("chat-request", ObservationType::Span);
//! request.set_input(&json!({"question": "Hi"}));
//!
//! let user = PropagatedAttributes {
//!     user_id: Some("user-42".into()),
//!     session_id: Some("session-7".into()),
//!     ..Default::default()
//! };
//! let answer = request
//!     .in_scope(propagate_attributes(user, async {
//!         // Child of `request`, with user and session set.
//!         let generation = tracer.start_observation("llm", ObservationType::Generation);
//!         generation.set_model("gpt-4o").set_input("Hi");
//!         let answer = call_llm("Hi").await;
//!         generation.set_output(&answer).set_usage_details(&json!({"input": 3, "output": 12}));
//!         generation.end();
//!         answer
//!     }))
//!     .await;
//!
//! request.set_output(&answer);
//! request.end();
//! tracer.shutdown().await; // flush before exiting
//! # Ok(())
//! # }
//! ```
//!
//! # Context
//!
//! The current observation lives in the OpenTelemetry [`Context`]:
//! [`Observation::in_scope`] makes an observation current while a future
//! runs, so observations started inside become its children and
//! [`Observation::current`], [`current_trace_id`] and
//! [`current_observation_id`] see it. The context follows the future, not
//! tasks spawned from it: wrap spawned futures with `in_scope` too.
//! [`propagate_attributes`] adds user, session, tags, metadata, version, trace
//! name, environment or prompt to every span of a scope, optionally as W3C
//! baggage for other services. [`TraceContext`] continues a trace whose ID
//! comes from elsewhere.
//!
//! # Export
//!
//! Finished spans are queued and exported every
//! [`flush_interval`](TracingOptions::flush_interval) or every
//! [`flush_at`](TracingOptions::flush_at) spans. Call
//! [`LangfuseTracer::flush`] in short-lived programs and
//! [`LangfuseTracer::shutdown`] before exiting. By default only Langfuse
//! observations, `gen_ai.*` spans and known LLM instrumentations are exported
//! ([`is_default_export_span`]); set
//! [`should_export_span`](TracingOptions::should_export_span) to change it.
//! [`sample_rate`](TracingOptions::sample_rate) keeps a share of traces,
//! [`mask`](TracingOptions::mask) redacts input, output and metadata, and
//! [`enabled`](TracingOptions::enabled) turns everything into no-ops.
//!
//! # Other instrumentation and the `tracing` crate
//!
//! [`LangfuseTracer::provider`] is a regular OpenTelemetry tracer provider:
//! spans from other libraries using it join the same traces. To record
//! `#[tracing::instrument]` spans, bridge them with `tracing-opentelemetry`
//! and accept them in `should_export_span` (their scope is the tracer name you
//! choose):
//!
//! ```rust,ignore
//! use opentelemetry::trace::TracerProvider as _;
//! use tracing_subscriber::layer::SubscriberExt;
//!
//! let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer.provider().tracer("my-app"));
//! tracing::subscriber::set_global_default(tracing_subscriber::registry().with(otel_layer))?;
//! ```
//!
//! [`opentelemetry`] and [`opentelemetry_sdk`] are re-exported so the types
//! of this module match the versions the SDK uses.
//!
//! [`Context`]: opentelemetry::Context

pub mod attributes;
mod exporter;
mod observation;
mod options;
mod processor;
mod propagation;
mod tracer;

pub use observation::{
    Level, Observation, ObservationType, TraceContext, current_observation_id, current_trace_id,
};
pub use options::{
    ENV_FLUSH_AT, ENV_FLUSH_INTERVAL, ENV_MAX_BATCH_SIZE_BYTES, ENV_RELEASE, ENV_SAMPLE_RATE,
    ENV_TRACING_ENABLED, ENV_TRACING_ENVIRONMENT, ExportFilter, MaskFn, TracingOptions,
};
pub use processor::{
    KNOWN_LLM_INSTRUMENTATION_SCOPE_PREFIXES, LANGFUSE_TRACER_NAME, is_default_export_span,
    is_genai_span, is_known_llm_instrumentor, is_langfuse_span,
};
pub use propagation::{PromptLink, PropagatedAttributes, propagate_attributes};
pub use tracer::LangfuseTracer;

pub use opentelemetry;
pub use opentelemetry_sdk;
