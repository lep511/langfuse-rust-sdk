//! Span processor feeding Langfuse, and the default export filters.

use std::collections::HashMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use opentelemetry::baggage::BaggageExt;
use opentelemetry::trace::{Span as _, SpanId};
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{IdGenerator, Span, SpanData, SpanProcessor};
use tokio::sync::mpsc;

use super::attributes::{self as attr, string_attribute};
use super::exporter::{Command, ExportTask, export_path};
use super::options::{ExportFilter, MaskFn, TracingOptions, otel_sdk_disabled_warning};
use super::propagation::{PropagatedState, TRACE_ID_BAGGAGE_KEY, inherited_attributes};
use crate::LangfuseClient;
use crate::error::LangfuseError;
use crate::skills::serialize_skills;

/// Instrumentation scope of the spans created by this SDK.
pub const LANGFUSE_TRACER_NAME: &str = "langfuse-sdk";

/// Instrumentation scopes (and their `.`-separated descendants) exported by
/// [`is_default_export_span`].
pub const KNOWN_LLM_INSTRUMENTATION_SCOPE_PREFIXES: &[&str] = &[
    LANGFUSE_TRACER_NAME,
    "agent_framework",
    "autogen-core",
    "ai",
    "haystack",
    "langsmith",
    "litellm",
    "openinference",
    "opentelemetry.instrumentation.agno",
    "opentelemetry.instrumentation.alephalpha",
    "opentelemetry.instrumentation.anthropic",
    "opentelemetry.instrumentation.bedrock",
    "opentelemetry.instrumentation.cohere",
    "opentelemetry.instrumentation.crewai",
    "opentelemetry.instrumentation.google_generativeai",
    "opentelemetry.instrumentation.groq",
    "opentelemetry.instrumentation.haystack",
    "opentelemetry.instrumentation.langchain",
    "opentelemetry.instrumentation.llamaindex",
    "opentelemetry.instrumentation.mistralai",
    "opentelemetry.instrumentation.ollama",
    "opentelemetry.instrumentation.openai",
    "opentelemetry.instrumentation.openai_agents",
    "opentelemetry.instrumentation.openai_v2",
    "opentelemetry.instrumentation.replicate",
    "opentelemetry.instrumentation.sagemaker",
    "opentelemetry.instrumentation.together",
    "opentelemetry.instrumentation.transformers",
    "opentelemetry.instrumentation.vertexai",
    "opentelemetry.instrumentation.voyageai",
    "opentelemetry.instrumentation.watsonx",
    "opentelemetry.instrumentation.writer",
    "pydantic-ai",
    "strands-agents",
    "vllm",
];

/// Spans of these `(scope, name)` pairs never become application roots when
/// they have a parent (they wrap a request of the real root).
const APP_ROOT_INELIGIBLE_SPANS: &[(&str, &str)] = &[("litellm", "raw_gen_ai_request")];

/// Whether the span was created by this SDK.
pub fn is_langfuse_span(span: &SpanData) -> bool {
    span.instrumentation_scope.name() == LANGFUSE_TRACER_NAME
}

/// Whether the span has a `gen_ai.*` semantic-convention attribute.
pub fn is_genai_span(span: &SpanData) -> bool {
    span.attributes
        .iter()
        .any(|kv| kv.key.as_str().starts_with("gen_ai."))
}

/// Whether the span comes from a known LLM instrumentation scope (see
/// [`KNOWN_LLM_INSTRUMENTATION_SCOPE_PREFIXES`]).
pub fn is_known_llm_instrumentor(span: &SpanData) -> bool {
    let scope = span.instrumentation_scope.name();
    KNOWN_LLM_INSTRUMENTATION_SCOPE_PREFIXES
        .iter()
        .any(|prefix| {
            scope == *prefix
                || scope
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// The default export filter: Langfuse observations, `gen_ai.*` spans and
/// spans of known LLM instrumentations. Other spans (HTTP servers, database
/// clients, ...) are dropped.
pub fn is_default_export_span(span: &SpanData) -> bool {
    is_langfuse_span(span) || is_genai_span(span) || is_known_llm_instrumentor(span)
}

fn is_app_root_eligible(span: &SpanData) -> bool {
    span.parent_span_id == SpanId::INVALID
        || !APP_ROOT_INELIGIBLE_SPANS
            .iter()
            .any(|(scope, name)| span.instrumentation_scope.name() == *scope && span.name == *name)
}

/// The Langfuse span processor, for a tracer provider you build yourself.
///
/// [`LangfuseTracer::new`](super::LangfuseTracer::new) creates one for its own
/// provider. To send Langfuse observations through a provider that also
/// feeds other backends, create the processor, add a clone to your provider
/// and build the tracer on that provider with
/// [`LangfuseTracer::with_provider`](super::LangfuseTracer::with_provider):
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::opentelemetry_sdk::trace::SdkTracerProvider;
/// use langfuse_rust_sdk::otel::{LangfuseSpanProcessor, LangfuseTracer, TracingOptions};
/// use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
///
/// # async fn example() -> Result<(), langfuse_rust_sdk::LangfuseError> {
/// let client = LangfuseClient::new(ClientConfig::from_env()?)?;
/// let processor = LangfuseSpanProcessor::new(&client, TracingOptions::from_env()?)?;
/// let provider = SdkTracerProvider::builder()
///     .with_span_processor(processor.clone())
///     // .with_batch_exporter(other_backend)
///     .build();
/// let tracer = LangfuseTracer::with_provider(&processor, provider.clone())?;
/// # let _ = tracer;
/// # Ok(())
/// # }
/// ```
///
/// The processor propagates trace attributes to new spans
/// ([`propagate_attributes`](super::propagate_attributes)), marks application
/// roots, filters finished spans with
/// [`should_export_span`](super::TracingOptions::should_export_span) and
/// exports them in batches from a Tokio task (so, with tracing enabled, it
/// must be created inside a runtime). Clones share that task; spans of every
/// provider the processor is added to go to the same project.
#[derive(Clone)]
pub struct LangfuseSpanProcessor {
    inner: Arc<Inner>,
}

struct Inner {
    public_key: String,
    should_export: Option<ExportFilter>,
    /// Configured skills (JSON), attached where the context asks for them.
    skills: Option<String>,
    /// `None` when tracing is disabled.
    commands: Option<mpsc::Sender<Command>>,
    /// Resource of the tracer provider, read by the exporter.
    resource: Arc<OnceLock<Resource>>,
    /// Whether each started, unfinished span is expected to be exported.
    expected: Mutex<HashMap<SpanId, bool>>,
    queue_full_warned: AtomicBool,
    /// Settings of the tracers built on this processor.
    settings: TracerSettings,
}

/// What a [`LangfuseTracer`](super::LangfuseTracer) takes from the options of
/// its processor.
pub(crate) struct TracerSettings {
    pub(crate) client: LangfuseClient,
    pub(crate) environment: Option<String>,
    pub(crate) release: Option<String>,
    pub(crate) mask: Option<MaskFn>,
    pub(crate) sample_rate: f64,
    pub(crate) id_generator: Option<Arc<dyn IdGenerator>>,
}

impl fmt::Debug for LangfuseSpanProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseSpanProcessor")
            .field("public_key", &self.inner.public_key)
            .field("enabled", &self.inner.commands.is_some())
            .finish_non_exhaustive()
    }
}

impl LangfuseSpanProcessor {
    /// Create a processor exporting through `client` with `options`, and start
    /// its export task.
    ///
    /// Returns [`LangfuseError::Config`] for invalid options or when called
    /// outside a Tokio runtime with tracing enabled. With
    /// [`enabled`](super::TracingOptions::enabled) off, the processor exports
    /// nothing and the tracers built on it record nothing.
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
            skills,
            compression,
            traces_export_path,
            mask_otel_spans,
            id_generator,
            span_exporter,
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
        if let Some(warning) = otel_sdk_disabled_warning(|name| std::env::var(name).ok(), enabled) {
            tracing::warn!("{warning}");
        }

        let resource = Arc::new(OnceLock::new());
        let commands = if enabled {
            let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
                LangfuseError::Config(
                    "LangfuseTracer and LangfuseSpanProcessor must be created inside a Tokio runtime".into(),
                )
            })?;
            let (sender, receiver) = mpsc::channel(max_queue_size);
            let task = ExportTask {
                client: client.clone(),
                media_upload,
                resource: Arc::clone(&resource),
                flush_at,
                flush_interval,
                max_batch_bytes,
                compression,
                export_path: export_path(traces_export_path.as_deref()),
                mask: mask_otel_spans,
                exporter: span_exporter,
                exporter_has_resource: false,
            };
            runtime.spawn(task.run(receiver));
            Some(sender)
        } else {
            None
        };

        Ok(Self {
            inner: Arc::new(Inner {
                public_key: client.public_key().to_owned(),
                should_export: should_export_span,
                skills: skills.as_deref().and_then(serialize_skills),
                commands,
                resource,
                expected: Mutex::new(HashMap::new()),
                queue_full_warned: AtomicBool::new(false),
                settings: TracerSettings {
                    client: client.clone(),
                    environment,
                    release,
                    mask,
                    sample_rate,
                    id_generator,
                },
            }),
        })
    }

    /// Whether tracing is enabled (spans are exported).
    pub fn is_enabled(&self) -> bool {
        self.inner.commands.is_some()
    }

    /// The public key of the project the spans are exported to.
    pub fn public_key(&self) -> &str {
        &self.inner.public_key
    }

    pub(crate) fn settings(&self) -> &TracerSettings {
        &self.inner.settings
    }

    /// The channel to the export task (`None` when disabled).
    pub(crate) fn commands(&self) -> Option<&mpsc::Sender<Command>> {
        self.inner.commands.as_ref()
    }

    /// Whether a tracer provider was built with this processor (it received
    /// the provider's resource).
    pub(crate) fn is_registered(&self) -> bool {
        self.inner.resource.get().is_some()
    }
}

impl Inner {
    /// Spans of this SDK tagged with another project's key belong to another
    /// tracer and must not leak into this project.
    fn is_foreign_project_span(&self, span: &SpanData) -> bool {
        is_langfuse_span(span)
            && span
                .instrumentation_scope
                .attributes()
                .find(|kv| kv.key.as_str() == "public_key")
                .is_none_or(|kv| kv.value.as_str() != self.public_key)
    }

    fn accepts(&self, span: &SpanData) -> bool {
        if self.is_foreign_project_span(span) {
            return false;
        }
        match &self.should_export {
            None => is_default_export_span(span),
            Some(filter) => match catch_unwind(AssertUnwindSafe(|| filter(span))) {
                Ok(accepted) => accepted,
                Err(_) => {
                    tracing::error!(span = %span.name, "should_export_span panicked; span dropped");
                    false
                }
            },
        }
    }

    fn expected(&self) -> std::sync::MutexGuard<'_, HashMap<SpanId, bool>> {
        self.expected.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SpanProcessor for LangfuseSpanProcessor {
    fn on_start(&self, span: &mut Span, cx: &Context) {
        let this = &*self.inner;
        if this.commands.is_none() {
            return;
        }
        let mut inherited = inherited_attributes(cx);
        let Some(data) = span.exported_data() else {
            return;
        };
        // A span that names its own prompt keeps it.
        if string_attribute(&data.attributes, attr::OBSERVATION_PROMPT_NAME).is_some() {
            inherited.retain(|kv| {
                kv.key.as_str() != attr::OBSERVATION_PROMPT_NAME
                    && kv.key.as_str() != attr::OBSERVATION_PROMPT_VERSION
            });
        }
        // Skills set on the span win over propagated ones, which win over the
        // configured ones.
        let has_key = |attributes: &[KeyValue]| {
            attributes
                .iter()
                .any(|kv| kv.key.as_str() == attr::OBSERVATION_SKILLS_AVAILABLE)
        };
        if has_key(&data.attributes) {
            inherited.retain(|kv| kv.key.as_str() != attr::OBSERVATION_SKILLS_AVAILABLE);
        } else if let Some(skills) = &this.skills
            && !has_key(&inherited)
            && !this.is_foreign_project_span(&data)
            && cx
                .get::<PropagatedState>()
                .is_some_and(PropagatedState::attaches_skills)
        {
            inherited.push(KeyValue::new(
                attr::OBSERVATION_SKILLS_AVAILABLE,
                skills.clone(),
            ));
        }
        if !inherited.is_empty() {
            span.set_attributes(inherited);
        }

        // Mark the first exported span of the application in its trace, so
        // Langfuse finds a root even when parent spans are filtered out.
        let Some(data) = span.exported_data() else {
            return;
        };
        let expected = this.accepts(&data);
        let span_id = data.span_context.span_id();
        let trace_id = data.span_context.trace_id().to_string();
        let claimed_by_parent = cx
            .baggage()
            .get(TRACE_ID_BAGGAGE_KEY)
            .is_some_and(|claimed| claimed.as_str().eq_ignore_ascii_case(&trace_id));
        let mark = {
            let mut map = this.expected();
            let parent_expected = map.get(&data.parent_span_id).copied().unwrap_or(false);
            map.insert(span_id, expected);
            expected && is_app_root_eligible(&data) && !parent_expected && !claimed_by_parent
        };
        if mark {
            span.set_attribute(KeyValue::new(attr::IS_APP_ROOT, true));
        }
    }

    fn on_end(&self, span: SpanData) {
        let this = &*self.inner;
        let Some(commands) = &this.commands else {
            return;
        };
        this.expected().remove(&span.span_context.span_id());
        if !this.accepts(&span) {
            tracing::debug!(span = %span.name, scope = span.instrumentation_scope.name(), "span not exported (should_export_span)");
            return;
        }
        match commands.try_send(Command::Span(Box::new(span))) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                if !this.queue_full_warned.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        "Langfuse span queue is full; dropping spans (raise max_queue_size or flush more often)"
                    );
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!("Langfuse tracer is shut down; span dropped");
            }
        }
    }

    /// Requests an export without waiting; use
    /// [`LangfuseTracer::flush`](super::LangfuseTracer::flush) to wait for it.
    fn force_flush(&self) -> OTelSdkResult {
        if let Some(commands) = &self.inner.commands {
            let _ = commands.try_send(Command::Flush(None));
        }
        Ok(())
    }

    /// Requests the final export without waiting; use
    /// [`LangfuseTracer::shutdown`](super::LangfuseTracer::shutdown) to wait.
    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        if let Some(commands) = &self.inner.commands {
            let _ = commands.try_send(Command::Shutdown(None));
        }
        Ok(())
    }

    fn set_resource(&mut self, resource: &Resource) {
        let _ = self.inner.resource.set(resource.clone());
    }
}
