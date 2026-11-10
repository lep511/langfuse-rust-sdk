//! Background export of finished spans to `POST /api/public/otel/v1/traces`
//! (or a custom exporter), with the export-stage hooks: media upload and
//! [`mask_otel_spans`](super::TracingOptions::mask_otel_spans).

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::BoxFuture;
use opentelemetry::trace::{SpanId, SpanKind, Status, TraceId};
use opentelemetry::{Array, InstrumentationScope, KeyValue, Value as OtelValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SpanData, SpanExporter};
use tokio::sync::{mpsc, oneshot};

use super::attributes::dedupe_last_wins;
use super::options::{Compression, SpanMaskFn};
use crate::LangfuseClient;
use crate::api::ingestion::OTEL_TRACES_PATH;
use crate::media::{MediaTarget, extract_from_attribute, field_for_attribute};
use crate::models::ingestion::{
    IngestTracesRequest, OtelAnyValue, OtelInstrumentationScope, OtelKeyValue, OtelResource,
    OtelResourceSpan, OtelScopeSpan, OtelSpan, OtelSpanEvent, OtelSpanLink, OtelStatus,
};

/// A finished span of an export batch, as passed to
/// [`TracingOptions::mask_otel_spans`](super::TracingOptions::mask_otel_spans).
///
/// Everything can be read ([`span`](Self::span) gives the whole
/// [`SpanData`]), but only the attributes can change, as in the Python SDK:
/// IDs, name, parent, events, links, scope and resource stay as they are.
/// The attributes come de-duplicated (one entry per key) and already hold the
/// media reference strings of uploaded media. Attributes left with an empty
/// key are dropped.
pub struct MaskableSpan {
    span: SpanData,
    resource: Option<Resource>,
}

impl fmt::Debug for MaskableSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MaskableSpan")
            .field("name", &self.span.name)
            .field("trace_id", &self.trace_id())
            .field("span_id", &self.span_id())
            .field("attributes", &self.span.attributes)
            .finish_non_exhaustive()
    }
}

impl MaskableSpan {
    /// The span as it would be exported.
    pub fn span(&self) -> &SpanData {
        &self.span
    }

    /// Trace ID of the span.
    pub fn trace_id(&self) -> TraceId {
        self.span.span_context.trace_id()
    }

    /// ID of the span.
    pub fn span_id(&self) -> SpanId {
        self.span.span_context.span_id()
    }

    /// ID of the parent span, if any.
    pub fn parent_span_id(&self) -> Option<SpanId> {
        (self.span.parent_span_id != SpanId::INVALID).then_some(self.span.parent_span_id)
    }

    /// Name of the span.
    pub fn name(&self) -> &str {
        &self.span.name
    }

    /// Instrumentation scope (name, version and attributes) of the span.
    pub fn instrumentation_scope(&self) -> &InstrumentationScope {
        &self.span.instrumentation_scope
    }

    /// Resource of the tracer provider (`service.name`, `langfuse.environment`, ...).
    pub fn resource(&self) -> Option<&Resource> {
        self.resource.as_ref()
    }

    /// The attributes that will be exported.
    pub fn attributes(&self) -> &[KeyValue] {
        &self.span.attributes
    }

    /// The attributes, to edit freely. When a key appears more than once, the
    /// last entry is exported.
    pub fn attributes_mut(&mut self) -> &mut Vec<KeyValue> {
        &mut self.span.attributes
    }

    /// The value of attribute `key`, if present.
    pub fn attribute(&self, key: &str) -> Option<&OtelValue> {
        self.span
            .attributes
            .iter()
            .rev()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &kv.value)
    }

    /// Set an attribute, replacing any with the same key.
    pub fn set_attribute(&mut self, attribute: KeyValue) {
        self.span.attributes.retain(|kv| kv.key != attribute.key);
        self.span.attributes.push(attribute);
    }

    /// Remove attribute `key`; returns its value if it was present.
    pub fn remove_attribute(&mut self, key: &str) -> Option<OtelValue> {
        let mut removed = None;
        self.span.attributes.retain(|kv| {
            if kv.key.as_str() == key {
                removed = Some(kv.value.clone());
                false
            } else {
                true
            }
        });
        removed
    }
}

/// Apply `mask` to a batch; `None` when it panicked (the batch is dropped).
fn mask_spans(
    mask: &SpanMaskFn,
    spans: Vec<SpanData>,
    resource: Option<&Resource>,
) -> Option<Vec<SpanData>> {
    let mut batch: Vec<MaskableSpan> = spans
        .into_iter()
        .filter(|span| span.span_context.is_valid())
        .map(|span| MaskableSpan {
            span,
            resource: resource.cloned(),
        })
        .collect();
    if catch_unwind(AssertUnwindSafe(|| mask(&mut batch))).is_err() {
        tracing::error!(
            spans = batch.len(),
            "mask_otel_spans panicked; export batch dropped"
        );
        return None;
    }
    Some(
        batch
            .into_iter()
            .map(|MaskableSpan { mut span, .. }| {
                span.attributes.retain(|kv| !kv.key.as_str().is_empty());
                span.attributes = dedupe_last_wins(span.attributes);
                span
            })
            .collect(),
    )
}

/// A [`SpanExporter`] set with
/// [`TracingOptions::with_span_exporter`](super::TracingOptions::with_span_exporter),
/// shared by clones of the options.
#[derive(Clone)]
pub struct SharedSpanExporter(Arc<dyn DynSpanExporter>);

impl SharedSpanExporter {
    /// Wrap `exporter`.
    pub fn new<E: SpanExporter + 'static>(exporter: E) -> Self {
        Self(Arc::new(Locked(tokio::sync::Mutex::new(exporter))))
    }
}

impl fmt::Debug for SharedSpanExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SharedSpanExporter").field(&self.0).finish()
    }
}

/// Object-safe [`SpanExporter`]; the export task is its only caller.
trait DynSpanExporter: Send + Sync + fmt::Debug {
    fn export(&self, batch: Vec<SpanData>) -> BoxFuture<'_, OTelSdkResult>;
    fn set_resource<'a>(&'a self, resource: &'a Resource) -> BoxFuture<'a, ()>;
    fn force_flush(&self) -> BoxFuture<'_, OTelSdkResult>;
    fn shutdown(&self) -> BoxFuture<'_, OTelSdkResult>;
}

/// `set_resource` takes `&mut self`, so the exporter sits behind an async
/// mutex (uncontended: one export task per tracer).
#[derive(Debug)]
struct Locked<E>(tokio::sync::Mutex<E>);

impl<E: SpanExporter + 'static> DynSpanExporter for Locked<E> {
    fn export(&self, batch: Vec<SpanData>) -> BoxFuture<'_, OTelSdkResult> {
        Box::pin(async move { self.0.lock().await.export(batch).await })
    }

    fn set_resource<'a>(&'a self, resource: &'a Resource) -> BoxFuture<'a, ()> {
        Box::pin(async move { self.0.lock().await.set_resource(resource) })
    }

    fn force_flush(&self) -> BoxFuture<'_, OTelSdkResult> {
        Box::pin(async move { self.0.lock().await.force_flush() })
    }

    fn shutdown(&self) -> BoxFuture<'_, OTelSdkResult> {
        Box::pin(async move { self.0.lock().await.shutdown() })
    }
}

/// Message from the processor and the tracer to the export task.
pub(crate) enum Command {
    Span(Box<SpanData>),
    /// Export everything queued; acknowledge when done.
    Flush(Option<oneshot::Sender<()>>),
    /// Export everything queued and stop.
    Shutdown(Option<oneshot::Sender<()>>),
}

pub(crate) struct ExportTask {
    /// Sends the spans and uploads media found in them.
    pub(crate) client: LangfuseClient,
    pub(crate) media_upload: bool,
    pub(crate) resource: Arc<OnceLock<Resource>>,
    pub(crate) flush_at: usize,
    pub(crate) flush_interval: Duration,
    pub(crate) max_batch_bytes: usize,
    pub(crate) compression: Compression,
    /// Path of the OTLP endpoint, relative to the base URL.
    pub(crate) export_path: String,
    pub(crate) mask: Option<SpanMaskFn>,
    /// Receives the spans instead of the Langfuse endpoint.
    pub(crate) exporter: Option<SharedSpanExporter>,
    /// Whether `exporter` was given the provider's resource.
    pub(crate) exporter_has_resource: bool,
}

/// The export path of `configured` (`TracingOptions::traces_export_path`).
pub(crate) fn export_path(configured: Option<&str>) -> String {
    match configured.map(|path| path.trim().trim_start_matches('/')) {
        Some(path) if !path.is_empty() => path.to_owned(),
        _ => OTEL_TRACES_PATH.trim_start_matches('/').to_owned(),
    }
}

impl ExportTask {
    pub(crate) async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut queued: Vec<SpanData> = Vec::new();
        let mut ticker = tokio::time::interval(self.flush_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await; // the first tick completes immediately
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(Command::Span(span)) => {
                        queued.push(*span);
                        if queued.len() >= self.flush_at {
                            self.export(std::mem::take(&mut queued)).await;
                        }
                    }
                    Some(Command::Flush(ack)) => {
                        self.export(std::mem::take(&mut queued)).await;
                        if let Some(exporter) = &self.exporter
                            && let Err(err) = exporter.0.force_flush().await
                        {
                            tracing::warn!(error = %err, "span exporter failed to flush");
                        }
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                    }
                    Some(Command::Shutdown(ack)) => {
                        // Spans queued before the shutdown request still go out.
                        while let Ok(Command::Span(span)) = commands.try_recv() {
                            queued.push(*span);
                        }
                        self.export(std::mem::take(&mut queued)).await;
                        self.shutdown_exporter().await;
                        commands.close();
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                        return;
                    }
                    None => {
                        self.export(std::mem::take(&mut queued)).await;
                        self.shutdown_exporter().await;
                        return;
                    }
                },
                _ = ticker.tick() => {
                    if !queued.is_empty() {
                        self.export(std::mem::take(&mut queued)).await;
                    }
                }
            }
        }
    }

    async fn shutdown_exporter(&self) {
        if let Some(exporter) = &self.exporter
            && let Err(err) = exporter.0.shutdown().await
        {
            tracing::warn!(error = %err, "span exporter failed to shut down");
        }
    }

    async fn export(&mut self, mut spans: Vec<SpanData>) {
        if spans.is_empty() {
            return;
        }
        for span in &mut spans {
            integer_usage_counts(&mut span.attributes);
        }
        if self.media_upload || self.mask.is_some() {
            for span in &mut spans {
                span.attributes = dedupe_last_wins(std::mem::take(&mut span.attributes));
            }
        }
        if self.media_upload {
            spans.iter_mut().for_each(|span| self.upload_media(span));
        }
        if let Some(mask) = &self.mask {
            match mask_spans(mask, spans, self.resource.get()) {
                Some(masked) => spans = masked,
                None => return,
            }
            if spans.is_empty() {
                return;
            }
        }
        if let Some(exporter) = &self.exporter {
            if let Some(resource) = self.resource.get().filter(|_| !self.exporter_has_resource) {
                exporter.0.set_resource(resource).await;
                self.exporter_has_resource = true;
            }
            let count = spans.len();
            match exporter.0.export(spans).await {
                Ok(()) => tracing::debug!(spans = count, "exported spans with the custom exporter"),
                Err(err) => {
                    tracing::warn!(spans = count, error = %err, "custom span exporter failed")
                }
            }
            return;
        }
        let resource = resource_attributes(self.resource.get());
        let gzip = self.compression == Compression::Gzip;
        for batch in split_by_size(
            spans.iter().map(to_otel_span).collect(),
            &spans,
            self.max_batch_bytes,
        ) {
            let count: usize = batch.iter().map(|scope| scope.spans.len()).sum();
            let request = IngestTracesRequest {
                resource_spans: vec![OtelResourceSpan {
                    resource: Some(OtelResource {
                        attributes: (!resource.is_empty()).then(|| resource.clone()),
                    }),
                    scope_spans: batch,
                }],
            };
            let sent = self
                .client
                .ingestion
                .export_traces(&request, &self.export_path, gzip)
                .await;
            if let Err(err) = sent {
                tracing::warn!(spans = count, error = %err, "failed to export spans to Langfuse");
            } else {
                tracing::debug!(spans = count, "exported spans to Langfuse");
            }
        }
    }
}

impl ExportTask {
    /// Replace base64 media in the string attributes of `span` (already
    /// de-duplicated) by reference strings and queue the uploads, linked to
    /// the span's observation.
    fn upload_media(&self, span: &mut SpanData) {
        let trace_id = span.span_context.trace_id().to_string();
        let span_id = span.span_context.span_id().to_string();
        for kv in &mut span.attributes {
            let mut found = Vec::new();
            match &mut kv.value {
                OtelValue::String(text) => {
                    if let Some(rewritten) = extract_from_attribute(text.as_str(), &mut found) {
                        *text = rewritten.into();
                    }
                }
                OtelValue::Array(Array::String(items)) => {
                    for item in items.iter_mut() {
                        if let Some(rewritten) = extract_from_attribute(item.as_str(), &mut found) {
                            *item = rewritten.into();
                        }
                    }
                }
                _ => {}
            }
            if !found.is_empty() {
                let field = field_for_attribute(kv.key.as_str());
                for media in found {
                    self.client.queue_media_upload(
                        media,
                        MediaTarget::observation(&trace_id, &span_id, field),
                    );
                }
            }
        }
    }
}

/// Group spans by instrumentation scope, starting a new request whenever the
/// JSON size would exceed `max_bytes`.
fn split_by_size(
    converted: Vec<OtelSpan>,
    originals: &[SpanData],
    max_bytes: usize,
) -> Vec<Vec<OtelScopeSpan>> {
    let mut batches: Vec<Vec<OtelScopeSpan>> = Vec::new();
    let mut current: Vec<OtelScopeSpan> = Vec::new();
    let mut current_bytes = 0usize;
    for (span, original) in converted.into_iter().zip(originals) {
        let size = serde_json::to_vec(&span).map(|v| v.len()).unwrap_or(0);
        if !current.is_empty() && current_bytes + size > max_bytes {
            batches.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += size;
        let scope = to_otel_scope(original);
        match current
            .iter_mut()
            .find(|s| same_scope(s.scope.as_ref(), &scope))
        {
            Some(group) => group.spans.push(span),
            None => current.push(OtelScopeSpan {
                scope: Some(scope),
                spans: vec![span],
            }),
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn same_scope(a: Option<&OtelInstrumentationScope>, b: &OtelInstrumentationScope) -> bool {
    a.is_some_and(|a| {
        a.name == b.name
            && a.version == b.version
            && serde_json::to_value(&a.attributes).ok() == serde_json::to_value(&b.attributes).ok()
    })
}

fn resource_attributes(resource: Option<&Resource>) -> Vec<OtelKeyValue> {
    resource
        .map(|r| {
            r.iter()
                .map(|(key, value)| OtelKeyValue::new(key.as_str(), to_any_value(value)))
                .collect()
        })
        .unwrap_or_default()
}

fn to_otel_scope(span: &SpanData) -> OtelInstrumentationScope {
    let scope = &span.instrumentation_scope;
    let attributes: Vec<OtelKeyValue> = scope.attributes().map(to_key_value).collect();
    OtelInstrumentationScope {
        name: Some(scope.name().to_owned()),
        version: scope.version().map(str::to_owned),
        attributes: (!attributes.is_empty()).then_some(attributes),
    }
}

/// Prefix of the OpenTelemetry GenAI token counts (`gen_ai.usage.input_tokens`, ...).
const GEN_AI_USAGE_PREFIX: &str = "gen_ai.usage.";

/// Turn `gen_ai.usage.*` counts recorded as strings back into integers.
///
/// `tracing` records unsigned integers through `Debug`, so the
/// `tracing-opentelemetry` bridge turns the `u64` token counts of crates like
/// rig into strings; the OpenTelemetry GenAI conventions (and Langfuse's
/// mapping of them) expect integers.
fn integer_usage_counts(attributes: &mut [KeyValue]) {
    for kv in attributes {
        if !kv.key.as_str().starts_with(GEN_AI_USAGE_PREFIX) {
            continue;
        }
        if let OtelValue::String(text) = &kv.value
            && let Ok(count) = text.as_str().trim().parse::<i64>()
        {
            kv.value = OtelValue::I64(count);
        }
    }
}

pub(crate) fn to_otel_span(span: &SpanData) -> OtelSpan {
    let context = &span.span_context;
    let trace_state = context.trace_state().header();
    let attributes: Vec<OtelKeyValue> = dedupe_last_wins(span.attributes.clone())
        .iter()
        .map(to_key_value)
        .collect();
    let events: Vec<OtelSpanEvent> = span
        .events
        .events
        .iter()
        .map(|event| OtelSpanEvent {
            time_unix_nano: unix_nanos(event.timestamp),
            name: event.name.to_string(),
            attributes: non_empty(event.attributes.iter().map(to_key_value).collect()),
        })
        .collect();
    let links: Vec<OtelSpanLink> = span
        .links
        .links
        .iter()
        .map(|link| {
            let header = link.span_context.trace_state().header();
            OtelSpanLink {
                trace_id: link.span_context.trace_id().to_string(),
                span_id: link.span_context.span_id().to_string(),
                trace_state: (!header.is_empty()).then_some(header),
                attributes: non_empty(link.attributes.iter().map(to_key_value).collect()),
            }
        })
        .collect();
    let (code, message) = match &span.status {
        Status::Unset => (0, None),
        Status::Ok => (1, None),
        Status::Error { description } => (
            2,
            (!description.is_empty()).then(|| description.to_string()),
        ),
    };
    OtelSpan {
        parent_span_id: (span.parent_span_id != SpanId::INVALID)
            .then(|| span.parent_span_id.to_string()),
        trace_state: (!trace_state.is_empty()).then_some(trace_state),
        kind: Some(span_kind(&span.span_kind)),
        attributes: Some(attributes),
        events: non_empty(events),
        links: non_empty(links),
        status: Some(OtelStatus {
            code: Some(code),
            message,
        }),
        ..OtelSpan::new(
            context.trace_id().to_string(),
            context.span_id().to_string(),
            span.name.to_string(),
            unix_nanos(span.start_time),
            unix_nanos(span.end_time),
        )
    }
}

fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
    (!items.is_empty()).then_some(items)
}

fn span_kind(kind: &SpanKind) -> i64 {
    match kind {
        SpanKind::Internal => 1,
        SpanKind::Server => 2,
        SpanKind::Client => 3,
        SpanKind::Producer => 4,
        SpanKind::Consumer => 5,
    }
}

fn unix_nanos(time: SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .to_string()
}

fn to_key_value(kv: &KeyValue) -> OtelKeyValue {
    OtelKeyValue::new(kv.key.as_str(), to_any_value(&kv.value))
}

fn to_any_value(value: &OtelValue) -> OtelAnyValue {
    match value {
        OtelValue::Bool(v) => OtelAnyValue::bool(*v),
        OtelValue::I64(v) => OtelAnyValue::int(*v),
        OtelValue::F64(v) => OtelAnyValue::double(*v),
        OtelValue::String(v) => OtelAnyValue::string(v.as_str()),
        OtelValue::Array(array) => match array {
            Array::Bool(items) => OtelAnyValue::array(items.iter().map(|v| OtelAnyValue::bool(*v))),
            Array::I64(items) => OtelAnyValue::array(items.iter().map(|v| OtelAnyValue::int(*v))),
            Array::F64(items) => {
                OtelAnyValue::array(items.iter().map(|v| OtelAnyValue::double(*v)))
            }
            Array::String(items) => {
                OtelAnyValue::strings(items.iter().map(|v| v.as_str().to_owned()))
            }
            other => OtelAnyValue::string(other.to_string()),
        },
        other => OtelAnyValue::string(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_path_defaults_and_drops_leading_slashes() {
        assert_eq!(export_path(None), "api/public/otel/v1/traces");
        assert_eq!(export_path(Some("")), "api/public/otel/v1/traces");
        assert_eq!(export_path(Some(" / ")), "api/public/otel/v1/traces");
        assert_eq!(export_path(Some("//proxy/traces")), "proxy/traces");
        assert_eq!(export_path(Some("otel")), "otel");
    }
}
