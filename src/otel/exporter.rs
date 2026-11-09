//! Background export of finished spans to `POST /api/public/otel/v1/traces`.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use opentelemetry::trace::{SpanId, SpanKind, Status};
use opentelemetry::{Array, KeyValue, Value as OtelValue};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SpanData;
use tokio::sync::{mpsc, oneshot};

use super::attributes::dedupe_last_wins;
use crate::LangfuseClient;
use crate::media::{MediaTarget, extract_from_attribute, field_for_attribute};
use crate::models::ingestion::{
    IngestTracesRequest, OtelAnyValue, OtelInstrumentationScope, OtelKeyValue, OtelResource,
    OtelResourceSpan, OtelScopeSpan, OtelSpan, OtelSpanEvent, OtelSpanLink, OtelStatus,
};

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
}

impl ExportTask {
    pub(crate) async fn run(self, mut commands: mpsc::Receiver<Command>) {
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
                        commands.close();
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                        return;
                    }
                    None => {
                        self.export(std::mem::take(&mut queued)).await;
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

    async fn export(&self, mut spans: Vec<SpanData>) {
        if spans.is_empty() {
            return;
        }
        if self.media_upload {
            spans.iter_mut().for_each(|span| self.upload_media(span));
        }
        let resource = resource_attributes(self.resource.get());
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
            if let Err(err) = self.client.ingestion.ingest_traces(&request).await {
                tracing::warn!(spans = count, error = %err, "failed to export spans to Langfuse");
            } else {
                tracing::debug!(spans = count, "exported spans to Langfuse");
            }
        }
    }
}

impl ExportTask {
    /// Replace base64 media in the string attributes of `span` by reference
    /// strings and queue the uploads, linked to the span's observation.
    fn upload_media(&self, span: &mut SpanData) {
        let trace_id = span.span_context.trace_id().to_string();
        let span_id = span.span_context.span_id().to_string();
        span.attributes = dedupe_last_wins(std::mem::take(&mut span.attributes));
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
