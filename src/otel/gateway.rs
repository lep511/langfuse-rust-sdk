//! Trace context of the current observation as Langfuse AI Gateway headers
//! (see [`GatewayHeaders::from_context`]).

use opentelemetry::trace::TraceContextExt as _;
use opentelemetry::{Context, Value as OtelValue};

use super::attributes as attr;
use super::observation::CurrentObservation;
use super::propagation::inherited_attributes;
use crate::gateway::GatewayHeaders;

/// The gateway headers of `cx`: `traceparent`/`tracestate` of its span, its
/// propagated attributes (state and `langfuse_*` baggage), else the
/// environment of the current observation.
pub(crate) fn gateway_context_headers(cx: &Context) -> GatewayHeaders {
    let mut headers = GatewayHeaders::default();
    let span = cx.span();
    let span_context = span.span_context();
    if span_context.is_valid() {
        headers.traceparent = Some(format!(
            "00-{}-{}-{:02x}",
            span_context.trace_id(),
            span_context.span_id(),
            span_context.trace_flags().to_u8()
        ));
        let state = span_context.trace_state().header();
        if !state.is_empty() {
            headers.tracestate = Some(state);
        }
    }

    let metadata_prefix = format!("{}.", attr::TRACE_METADATA);
    for kv in inherited_attributes(cx) {
        let key = kv.key.as_str();
        match key {
            attr::TRACE_USER_ID => headers.user_id = Some(kv.value.as_str().into_owned()),
            attr::TRACE_SESSION_ID => headers.session_id = Some(kv.value.as_str().into_owned()),
            attr::TRACE_NAME => headers.trace_name = Some(kv.value.as_str().into_owned()),
            attr::ENVIRONMENT => headers.environment = Some(kv.value.as_str().into_owned()),
            attr::TRACE_TAGS => {
                if let OtelValue::Array(opentelemetry::Array::String(tags)) = &kv.value {
                    headers.tags = tags.iter().map(|t| t.as_str().to_owned()).collect();
                }
            }
            _ => {
                if let Some(name) = key.strip_prefix(&metadata_prefix) {
                    headers
                        .metadata
                        .insert(name.to_owned(), metadata_text(&kv.value.as_str()));
                }
            }
        }
    }

    if headers.environment.is_none()
        && let Some(current) = cx.get::<CurrentObservation>()
    {
        headers.environment = current.0.environment().map(str::to_owned);
    }
    headers
}

/// A propagated metadata value as gateway text: propagated values are JSON
/// (strings lose their quotes, other values stay JSON), baggage values raw.
fn metadata_text(value: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_values_lose_their_json_quotes() {
        assert_eq!(metadata_text("\"main\""), "main");
        assert_eq!(metadata_text("42"), "42");
        assert_eq!(metadata_text("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(metadata_text("raw baggage"), "raw baggage");
    }

    #[test]
    fn no_span_no_trace_context() {
        let headers = gateway_context_headers(&Context::new());
        assert_eq!(headers, GatewayHeaders::default());
    }
}
