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
use opentelemetry_sdk::trace::{Span, SpanData, SpanProcessor};
use tokio::sync::mpsc;

use super::attributes::{self as attr, string_attribute};
use super::exporter::Command;
use super::options::ExportFilter;
use super::propagation::{TRACE_ID_BAGGAGE_KEY, inherited_attributes};

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

/// Propagates trace attributes to new spans, filters finished spans and
/// queues them for export.
pub(crate) struct LangfuseSpanProcessor {
    public_key: String,
    should_export: Option<ExportFilter>,
    commands: mpsc::Sender<Command>,
    /// Resource of the tracer provider, read by the exporter.
    resource: Arc<OnceLock<Resource>>,
    /// Whether each started, unfinished span is expected to be exported.
    expected: Mutex<HashMap<SpanId, bool>>,
    queue_full_warned: AtomicBool,
}

impl fmt::Debug for LangfuseSpanProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseSpanProcessor")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl LangfuseSpanProcessor {
    pub(crate) fn new(
        public_key: String,
        should_export: Option<ExportFilter>,
        commands: mpsc::Sender<Command>,
        resource: Arc<OnceLock<Resource>>,
    ) -> Self {
        Self {
            public_key,
            should_export,
            commands,
            resource,
            expected: Mutex::new(HashMap::new()),
            queue_full_warned: AtomicBool::new(false),
        }
    }

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
        if !inherited.is_empty() {
            span.set_attributes(inherited);
        }

        // Mark the first exported span of the application in its trace, so
        // Langfuse finds a root even when parent spans are filtered out.
        let Some(data) = span.exported_data() else {
            return;
        };
        let expected = self.accepts(&data);
        let span_id = data.span_context.span_id();
        let trace_id = data.span_context.trace_id().to_string();
        let claimed_by_parent = cx
            .baggage()
            .get(TRACE_ID_BAGGAGE_KEY)
            .is_some_and(|claimed| claimed.as_str().eq_ignore_ascii_case(&trace_id));
        let mark = {
            let mut map = self.expected();
            let parent_expected = map.get(&data.parent_span_id).copied().unwrap_or(false);
            map.insert(span_id, expected);
            expected && is_app_root_eligible(&data) && !parent_expected && !claimed_by_parent
        };
        if mark {
            span.set_attribute(KeyValue::new(attr::IS_APP_ROOT, true));
        }
    }

    fn on_end(&self, span: SpanData) {
        self.expected().remove(&span.span_context.span_id());
        if !self.accepts(&span) {
            tracing::debug!(span = %span.name, scope = span.instrumentation_scope.name(), "span not exported (should_export_span)");
            return;
        }
        match self.commands.try_send(Command::Span(Box::new(span))) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                if !self.queue_full_warned.swap(true, Ordering::Relaxed) {
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
        let _ = self.commands.try_send(Command::Flush(None));
        Ok(())
    }

    /// Requests the final export without waiting; use
    /// [`LangfuseTracer::shutdown`](super::LangfuseTracer::shutdown) to wait.
    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        let _ = self.commands.try_send(Command::Shutdown(None));
        Ok(())
    }

    fn set_resource(&mut self, resource: &Resource) {
        let _ = self.resource.set(resource.clone());
    }
}
