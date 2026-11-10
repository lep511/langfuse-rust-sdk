//! Span attribute keys that Langfuse reads from OpenTelemetry spans.
//!
//! The SDK sets them for you; they are public so spans created by other
//! instrumentation can carry Langfuse data too.

use opentelemetry::{KeyValue, Value as OtelValue};
use serde::Serialize;
use serde_json::{Map, Value};

/// Trace name.
pub const TRACE_NAME: &str = "langfuse.trace.name";
/// User ID of the trace.
pub const TRACE_USER_ID: &str = "user.id";
/// Session ID of the trace.
pub const TRACE_SESSION_ID: &str = "session.id";
/// Trace tags (string array).
pub const TRACE_TAGS: &str = "langfuse.trace.tags";
/// Whether the trace is public.
pub const TRACE_PUBLIC: &str = "langfuse.trace.public";
/// Prefix of trace metadata keys (`langfuse.trace.metadata.<key>`).
pub const TRACE_METADATA: &str = "langfuse.trace.metadata";

/// Observation type (`span`, `generation`, `event`, `agent`, ...).
pub const OBSERVATION_TYPE: &str = "langfuse.observation.type";
/// Prefix of observation metadata keys (`langfuse.observation.metadata.<key>`).
pub const OBSERVATION_METADATA: &str = "langfuse.observation.metadata";
/// Observation level (`DEBUG`, `DEFAULT`, `WARNING`, `ERROR`).
pub const OBSERVATION_LEVEL: &str = "langfuse.observation.level";
/// Observation status message.
pub const OBSERVATION_STATUS_MESSAGE: &str = "langfuse.observation.status_message";
/// Observation input (JSON, or plain text for string inputs).
pub const OBSERVATION_INPUT: &str = "langfuse.observation.input";
/// Observation output (JSON, or plain text for string outputs).
pub const OBSERVATION_OUTPUT: &str = "langfuse.observation.output";
/// When a generation started producing output (JSON string).
pub const OBSERVATION_COMPLETION_START_TIME: &str = "langfuse.observation.completion_start_time";
/// Model name of a generation.
pub const OBSERVATION_MODEL: &str = "langfuse.observation.model.name";
/// Model parameters of a generation (JSON object).
pub const OBSERVATION_MODEL_PARAMETERS: &str = "langfuse.observation.model.parameters";
/// Usage details of a generation (JSON object, e.g. `{"input": 10}`).
pub const OBSERVATION_USAGE_DETAILS: &str = "langfuse.observation.usage_details";
/// Cost details of a generation (JSON object).
pub const OBSERVATION_COST_DETAILS: &str = "langfuse.observation.cost_details";
/// Name of the Langfuse prompt used by a generation.
pub const OBSERVATION_PROMPT_NAME: &str = "langfuse.observation.prompt.name";
/// Version of the Langfuse prompt used by a generation (integer).
pub const OBSERVATION_PROMPT_VERSION: &str = "langfuse.observation.prompt.version";
/// Skills available to the observation (JSON array of
/// [`SkillReference`](crate::skills::SkillReference)).
pub const OBSERVATION_SKILLS_AVAILABLE: &str = "langfuse.observation.skills_available";

/// Tracing environment (e.g. `production`).
pub const ENVIRONMENT: &str = "langfuse.environment";
/// Release of the application.
pub const RELEASE: &str = "langfuse.release";
/// Version of the code or component.
pub const VERSION: &str = "langfuse.version";

/// Experiment ID (set by [`LangfuseTracer::run_experiment`](super::LangfuseTracer::run_experiment)).
pub const EXPERIMENT_ID: &str = "langfuse.experiment.id";
/// Experiment run name.
pub const EXPERIMENT_NAME: &str = "langfuse.experiment.name";
/// Experiment description.
pub const EXPERIMENT_DESCRIPTION: &str = "langfuse.experiment.description";
/// Experiment metadata (JSON object).
pub const EXPERIMENT_METADATA: &str = "langfuse.experiment.metadata";
/// Dataset of the experiment items.
pub const EXPERIMENT_DATASET_ID: &str = "langfuse.experiment.dataset.id";
/// Experiment item ID: the dataset item ID, or a hash of the input of local items.
pub const EXPERIMENT_ITEM_ID: &str = "langfuse.experiment.item.id";
/// Version of the dataset the item was read at.
pub const EXPERIMENT_ITEM_VERSION: &str = "langfuse.experiment.item.version";
/// Expected output of the experiment item (JSON, or plain text for strings).
pub const EXPERIMENT_ITEM_EXPECTED_OUTPUT: &str = "langfuse.experiment.item.expected_output";
/// Metadata of the experiment item (JSON object).
pub const EXPERIMENT_ITEM_METADATA: &str = "langfuse.experiment.item.metadata";
/// ID of the observation running the task of the experiment item.
pub const EXPERIMENT_ITEM_ROOT_OBSERVATION_ID: &str =
    "langfuse.experiment.item.root_observation_id";
/// Environment of every span of an experiment.
pub const SDK_EXPERIMENT_ENVIRONMENT: &str = "sdk-experiment";

/// Internal: the span starts a trace whose ID was given explicitly.
pub const AS_ROOT: &str = "langfuse.internal.as_root";
/// Internal: the first exported span of the application in its trace.
pub const IS_APP_ROOT: &str = "langfuse.internal.is_app_root";

/// Largest integer that JavaScript represents exactly (`2^53 - 1`).
const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Convert integers that JavaScript cannot represent exactly into strings,
/// recursively, so the server does not round them.
pub(crate) fn js_safe(value: Value) -> Value {
    match value {
        Value::Number(n) => {
            let unsafe_int = n.as_u64().is_some_and(|v| v > JS_MAX_SAFE_INTEGER)
                || n.as_i64()
                    .is_some_and(|v| v.unsigned_abs() > JS_MAX_SAFE_INTEGER);
            if unsafe_int {
                Value::String(n.to_string())
            } else {
                Value::Number(n)
            }
        }
        Value::Array(items) => Value::Array(items.into_iter().map(js_safe).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, js_safe(v))).collect())
        }
        other => other,
    }
}

/// Serialize any value into JSON, logging (instead of failing) on error:
/// tracing calls never break the application.
pub(crate) fn to_json<T: Serialize + ?Sized>(value: &T, what: &str) -> Option<Value> {
    match serde_json::to_value(value) {
        Ok(json) => Some(json),
        Err(err) => {
            tracing::warn!(attribute = what, error = %err, "could not serialize observation data; skipped");
            None
        }
    }
}

/// Input/output encoding: strings as-is, anything else as compact JSON.
pub(crate) fn io_value(value: Value) -> String {
    match value {
        Value::String(text) => text,
        other => js_safe(other).to_string(),
    }
}

/// JSON encoding of one metadata value, strings included (so `"1"` and `1`
/// stay distinct).
pub(crate) fn metadata_value(value: Value) -> String {
    js_safe(value).to_string()
}

/// Flatten metadata into `<prefix>.<key>` attributes; a non-object value goes
/// to `<prefix>` itself. `null` values are skipped, so an update never erases
/// an earlier value.
pub(crate) fn flatten_metadata(prefix: &str, metadata: Value) -> Vec<(String, String)> {
    match metadata {
        Value::Null => Vec::new(),
        Value::Object(map) => flatten_map(prefix, map),
        other => vec![(prefix.to_owned(), metadata_value(other))],
    }
}

fn flatten_map(prefix: &str, map: Map<String, Value>) -> Vec<(String, String)> {
    map.into_iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (format!("{prefix}.{k}"), metadata_value(v)))
        .collect()
}

/// Keep the last value of each key, in order of last occurrence. The SDK
/// appends repeated `set_attribute` calls instead of replacing them.
pub(crate) fn dedupe_last_wins(attributes: Vec<KeyValue>) -> Vec<KeyValue> {
    let mut out: Vec<KeyValue> = Vec::with_capacity(attributes.len());
    for kv in attributes {
        if let Some(pos) = out.iter().position(|existing| existing.key == kv.key) {
            out.remove(pos);
        }
        out.push(kv);
    }
    out
}

/// The string value of attribute `key`, if present.
pub(crate) fn string_attribute<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .rev()
        .find(|kv| kv.key.as_str() == key)
        .and_then(|kv| match &kv.value {
            OtelValue::String(s) => Some(s.as_str()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strings_stay_raw_in_io_and_quoted_in_metadata() {
        assert_eq!(io_value(json!("hello")), "hello");
        assert_eq!(io_value(json!({"a": [1, "ü"]})), r#"{"a":[1,"ü"]}"#);
        assert_eq!(metadata_value(json!("hello")), r#""hello""#);
        assert_eq!(metadata_value(json!(123)), "123");
    }

    #[test]
    fn unsafe_integers_become_strings() {
        let value = json!({"ok": 9_007_199_254_740_991_u64, "big": 9_007_199_254_740_992_u64, "neg": -9_007_199_254_740_993_i64, "f": 1.5});
        assert_eq!(
            io_value(value),
            r#"{"big":"9007199254740992","f":1.5,"neg":"-9007199254740993","ok":9007199254740991}"#
        );
    }

    #[test]
    fn metadata_is_flattened_and_nulls_skipped() {
        let flat = flatten_metadata(
            OBSERVATION_METADATA,
            json!({"a": "x", "b": {"c": 1}, "skip": null}),
        );
        assert_eq!(
            flat,
            vec![
                (
                    "langfuse.observation.metadata.a".to_string(),
                    r#""x""#.to_string()
                ),
                (
                    "langfuse.observation.metadata.b".to_string(),
                    r#"{"c":1}"#.to_string()
                ),
            ]
        );
        assert_eq!(
            flatten_metadata(OBSERVATION_METADATA, json!(["x"])),
            vec![(
                "langfuse.observation.metadata".to_string(),
                r#"["x"]"#.to_string()
            )]
        );
        assert!(flatten_metadata(OBSERVATION_METADATA, Value::Null).is_empty());
    }

    #[test]
    fn dedupe_keeps_the_last_value() {
        let attrs = vec![
            KeyValue::new("a", 1),
            KeyValue::new("b", 2),
            KeyValue::new("a", 3),
        ];
        let deduped = dedupe_last_wins(attrs);
        assert_eq!(deduped, vec![KeyValue::new("b", 2), KeyValue::new("a", 3)]);
    }
}
