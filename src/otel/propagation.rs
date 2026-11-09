//! Trace-level attributes propagated to every span created in a scope.

use std::collections::BTreeMap;
use std::future::Future;

use opentelemetry::baggage::BaggageExt;
use opentelemetry::context::FutureExt;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Context, KeyValue, StringValue, Value as OtelValue};
use serde::Serialize;

use super::attributes::{self as attr, metadata_value, to_json};
use super::observation::{CurrentObservation, set_or_buffer};
use crate::models::prompts::Prompt;

/// Prefix of the baggage keys written with [`PropagatedAttributes::as_baggage`].
const BAGGAGE_PREFIX: &str = "langfuse_";
/// Baggage key with the trace ID of the observation made current: spans of the
/// same trace in other services are then not marked as application roots.
pub(crate) const TRACE_ID_BAGGAGE_KEY: &str = "langfuse_trace_id";
/// Longest propagated value; longer ones are dropped.
const MAX_VALUE_CHARS: usize = 200;
/// Longest environment name.
const MAX_ENVIRONMENT_CHARS: usize = 40;

/// Trace attributes applied to the current observation and to every span
/// started inside [`propagate_attributes`]. Every field is optional.
///
/// Values longer than 200 characters (40 for `environment`) are dropped with a
/// warning; `environment` must be lowercase letters, digits, `-` or `_` and
/// must not start with `langfuse`.
#[derive(Debug, Clone, Default)]
pub struct PropagatedAttributes {
    /// User of the trace.
    pub user_id: Option<String>,
    /// Session of the trace.
    pub session_id: Option<String>,
    /// Version of the code or component.
    pub version: Option<String>,
    /// Trace tags; merged with the tags of outer scopes.
    pub tags: Vec<String>,
    /// Trace name.
    pub trace_name: Option<String>,
    /// Environment, overriding the tracer default.
    pub environment: Option<String>,
    /// Trace metadata (JSON values); merged with the metadata of outer scopes.
    pub metadata: BTreeMap<String, serde_json::Value>,
    /// Langfuse prompt linked to every generation of the scope.
    pub prompt: Option<PromptLink>,
    /// Also write the attributes to W3C baggage, so they cross service
    /// boundaries with a baggage propagator. Baggage is sent in HTTP headers:
    /// do not use it for sensitive values.
    pub as_baggage: bool,
}

impl PropagatedAttributes {
    /// Set `metadata[key]` to `value` (anything serializable).
    pub fn with_metadata<T: Serialize + ?Sized>(
        mut self,
        key: impl Into<String>,
        value: &T,
    ) -> Self {
        if let Some(value) = to_json(value, "metadata") {
            self.metadata.insert(key.into(), value);
        }
        self
    }
}

/// Name and version of a Langfuse prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptLink {
    /// Prompt name.
    pub name: String,
    /// Prompt version.
    pub version: i64,
}

impl PromptLink {
    /// The link to `prompt`, or `None` for a fallback prompt (it exists only
    /// in the application, so there is nothing to link).
    pub fn from_prompt(prompt: &Prompt) -> Option<Self> {
        (!prompt.is_fallback()).then(|| Self {
            name: prompt.name().to_owned(),
            version: prompt.version(),
        })
    }
}

/// Run `future` with `attributes` applied to the current observation and to
/// every span started while it runs (including spans of other
/// instrumentation libraries on the same tracer provider).
///
/// Scopes nest: inner values override outer ones, and tags and metadata are
/// merged. Like every context in async Rust, the attributes follow the future,
/// not spawned tasks: wrap the spawned future too if needed.
///
/// ```rust,no_run
/// use langfuse_rust_sdk::otel::{PropagatedAttributes, propagate_attributes};
///
/// # async fn handle() {}
/// # async fn example() {
/// let attributes = PropagatedAttributes {
///     user_id: Some("user-42".into()),
///     session_id: Some("chat-7".into()),
///     tags: vec!["beta".into()],
///     ..Default::default()
/// };
/// propagate_attributes(attributes, handle()).await;
/// # }
/// ```
pub async fn propagate_attributes<F: Future>(
    attributes: PropagatedAttributes,
    future: F,
) -> F::Output {
    // The body runs when first polled, so the current context is the one of
    // whatever wraps the returned future, not the one at the call site.
    let cx = context_with(attributes, &Context::current());
    future.with_context(cx).await
}

/// Validated, merged propagated attributes stored in the OpenTelemetry context.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropagatedState {
    user_id: Option<String>,
    session_id: Option<String>,
    version: Option<String>,
    tags: Vec<String>,
    trace_name: Option<String>,
    environment: Option<String>,
    /// Serialized (JSON) values.
    metadata: BTreeMap<String, String>,
    prompt: Option<PromptLink>,
}

impl PropagatedState {
    /// Span attributes for this state.
    pub(crate) fn span_attributes(&self) -> Vec<KeyValue> {
        let mut out = Vec::new();
        let strings = [
            (attr::TRACE_USER_ID, &self.user_id),
            (attr::TRACE_SESSION_ID, &self.session_id),
            (attr::VERSION, &self.version),
            (attr::TRACE_NAME, &self.trace_name),
            (attr::ENVIRONMENT, &self.environment),
        ];
        for (key, value) in strings {
            if let Some(value) = value {
                out.push(KeyValue::new(key, value.clone()));
            }
        }
        if !self.tags.is_empty() {
            let tags: Vec<StringValue> = self.tags.iter().cloned().map(StringValue::from).collect();
            out.push(KeyValue::new(
                attr::TRACE_TAGS,
                OtelValue::Array(tags.into()),
            ));
        }
        for (key, value) in &self.metadata {
            out.push(KeyValue::new(
                format!("{}.{key}", attr::TRACE_METADATA),
                value.clone(),
            ));
        }
        if let Some(prompt) = &self.prompt {
            out.push(KeyValue::new(
                attr::OBSERVATION_PROMPT_NAME,
                prompt.name.clone(),
            ));
            out.push(KeyValue::new(
                attr::OBSERVATION_PROMPT_VERSION,
                prompt.version,
            ));
        }
        out
    }

    fn merge(&mut self, attributes: PropagatedAttributes) {
        let checked = |key: &str, value: Option<String>| value.filter(|v| valid_string(key, v));
        if let Some(v) = checked("user_id", attributes.user_id) {
            self.user_id = Some(v);
        }
        if let Some(v) = checked("session_id", attributes.session_id) {
            self.session_id = Some(v);
        }
        if let Some(v) = checked("version", attributes.version) {
            self.version = Some(v);
        }
        if let Some(v) = checked("trace_name", attributes.trace_name) {
            self.trace_name = Some(v);
        }
        if let Some(v) = attributes.environment.filter(|v| valid_environment(v)) {
            self.environment = Some(v);
        }
        for tag in attributes.tags {
            if valid_string("tags", &tag) && !self.tags.contains(&tag) {
                self.tags.push(tag);
            }
        }
        for (key, value) in attributes.metadata {
            if value.is_null() {
                tracing::warn!(key = %key, "propagated metadata value is null; dropped");
                continue;
            }
            let serialized = metadata_value(value);
            if valid_string(&format!("metadata.{key}"), &serialized) {
                self.metadata.insert(key, serialized);
            }
        }
        if let Some(prompt) = attributes.prompt {
            self.prompt = Some(prompt);
        }
    }

    /// Baggage entries for this state (`langfuse_<key>`).
    fn baggage(&self) -> Vec<KeyValue> {
        let mut out = Vec::new();
        let strings = [
            ("user_id", &self.user_id),
            ("session_id", &self.session_id),
            ("version", &self.version),
            ("trace_name", &self.trace_name),
            ("environment", &self.environment),
        ];
        for (key, value) in strings {
            if let Some(value) = value {
                out.push(KeyValue::new(
                    format!("{BAGGAGE_PREFIX}{key}"),
                    value.clone(),
                ));
            }
        }
        if !self.tags.is_empty() {
            out.push(KeyValue::new(
                format!("{BAGGAGE_PREFIX}tags"),
                self.tags.join(","),
            ));
        }
        for (key, value) in &self.metadata {
            out.push(KeyValue::new(
                format!("{BAGGAGE_PREFIX}metadata_{key}"),
                value.clone(),
            ));
        }
        if let Some(prompt) = &self.prompt {
            out.push(KeyValue::new(
                format!("{BAGGAGE_PREFIX}prompt_name"),
                prompt.name.clone(),
            ));
            out.push(KeyValue::new(
                format!("{BAGGAGE_PREFIX}prompt_version"),
                prompt.version.to_string(),
            ));
        }
        out
    }
}

/// `cx` extended with `attributes`, which are also applied to its current span.
fn context_with(attributes: PropagatedAttributes, cx: &Context) -> Context {
    let as_baggage = attributes.as_baggage;
    let mut state = cx.get::<PropagatedState>().cloned().unwrap_or_default();
    state.merge(attributes);

    // The current observation (or foreign span) gets the attributes now; spans
    // started later get them from the processor.
    let current = cx.get::<CurrentObservation>().map(|c| c.0.clone());
    set_or_buffer(current.as_ref(), &cx.span(), state.span_attributes());

    let mut out = cx.with_value(state.clone());
    if as_baggage {
        out = out.with_baggage(state.baggage());
    }
    out
}

/// Attributes a new span inherits from its parent context: the propagated
/// state and any `langfuse_*` baggage (state values win).
pub(crate) fn inherited_attributes(cx: &Context) -> Vec<KeyValue> {
    let mut merged: BTreeMap<String, OtelValue> = BTreeMap::new();
    for (key, (value, _)) in cx.baggage().iter() {
        let key = key.as_str();
        if key == TRACE_ID_BAGGAGE_KEY {
            continue;
        }
        if let Some(suffix) = key.strip_prefix(BAGGAGE_PREFIX)
            && let Some((span_key, value)) = from_baggage(suffix, value.as_str().to_owned())
        {
            merged.insert(span_key, value);
        }
    }
    if let Some(state) = cx.get::<PropagatedState>() {
        for kv in state.span_attributes() {
            merged.insert(kv.key.to_string(), kv.value);
        }
    }
    merged
        .into_iter()
        .map(|(k, v)| KeyValue::new(k, v))
        .collect()
}

/// Map one `langfuse_<suffix>` baggage entry to its span attribute.
fn from_baggage(suffix: &str, value: String) -> Option<(String, OtelValue)> {
    if let Some(key) = suffix.strip_prefix("metadata_") {
        return Some((format!("{}.{key}", attr::TRACE_METADATA), value.into()));
    }
    let span_key = match suffix {
        "user_id" => attr::TRACE_USER_ID,
        "session_id" => attr::TRACE_SESSION_ID,
        "version" => attr::VERSION,
        "trace_name" => attr::TRACE_NAME,
        "prompt_name" => attr::OBSERVATION_PROMPT_NAME,
        "environment" => {
            return valid_environment(&value).then(|| (attr::ENVIRONMENT.to_owned(), value.into()));
        }
        "prompt_version" => {
            return value
                .parse::<i64>()
                .ok()
                .map(|v| (attr::OBSERVATION_PROMPT_VERSION.to_owned(), v.into()));
        }
        "tags" => {
            let tags: Vec<StringValue> = value
                .split(',')
                .filter(|t| !t.is_empty())
                .map(|t| StringValue::from(t.to_owned()))
                .collect();
            return Some((attr::TRACE_TAGS.to_owned(), OtelValue::Array(tags.into())));
        }
        other => return Some((format!("{}.{other}", attr::TRACE_METADATA), value.into())),
    };
    Some((span_key.to_owned(), value.into()))
}

fn valid_string(key: &str, value: &str) -> bool {
    let chars = value.chars().count();
    if chars > MAX_VALUE_CHARS {
        tracing::warn!(
            key,
            chars,
            "propagated attribute is over 200 characters; dropped"
        );
        return false;
    }
    true
}

fn valid_environment(value: &str) -> bool {
    let valid = !value.is_empty()
        && value.len() <= MAX_ENVIRONMENT_CHARS
        && !value.starts_with("langfuse")
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if !valid {
        tracing::warn!(
            environment = value,
            "environment must be at most 40 lowercase letters, digits, '-' or '_' and not start with 'langfuse'; dropped"
        );
    }
    valid
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn attributes_of(state: &PropagatedState) -> BTreeMap<String, String> {
        state
            .span_attributes()
            .into_iter()
            .map(|kv| (kv.key.to_string(), kv.value.to_string()))
            .collect()
    }

    #[test]
    fn merging_overrides_values_and_unions_tags_and_metadata() {
        let mut state = PropagatedState::default();
        state.merge(PropagatedAttributes {
            user_id: Some("u1".into()),
            tags: vec!["a".into(), "b".into()],
            metadata: BTreeMap::from([("k1".to_string(), json!("v1"))]),
            ..Default::default()
        });
        state.merge(PropagatedAttributes {
            user_id: Some("u2".into()),
            tags: vec!["b".into(), "c".into()],
            metadata: BTreeMap::from([("k2".to_string(), json!(2))]),
            ..Default::default()
        });
        assert_eq!(state.user_id.as_deref(), Some("u2"));
        assert_eq!(state.tags, ["a", "b", "c"]);
        let attrs = attributes_of(&state);
        assert_eq!(attrs["langfuse.trace.metadata.k1"], r#""v1""#);
        assert_eq!(attrs["langfuse.trace.metadata.k2"], "2");
    }

    #[test]
    fn invalid_values_are_dropped() {
        let mut state = PropagatedState::default();
        state.merge(PropagatedAttributes {
            user_id: Some("x".repeat(201)),
            environment: Some("Prod".into()),
            metadata: BTreeMap::from([("big".to_string(), json!("y".repeat(300)))]),
            ..Default::default()
        });
        assert!(state.user_id.is_none());
        assert!(state.environment.is_none());
        assert!(state.metadata.is_empty());

        for env in ["langfuse-prod", "a b", ""] {
            assert!(!valid_environment(env), "{env}");
        }
        for env in ["production", "dev_2", "eu-west"] {
            assert!(valid_environment(env), "{env}");
        }
    }

    #[test]
    fn baggage_round_trips_to_span_attributes() {
        let mut state = PropagatedState::default();
        state.merge(PropagatedAttributes {
            user_id: Some("u1".into()),
            tags: vec!["a".into(), "b".into()],
            environment: Some("staging".into()),
            metadata: BTreeMap::from([("team".to_string(), json!("search"))]),
            prompt: Some(PromptLink {
                name: "p".into(),
                version: 3,
            }),
            ..Default::default()
        });
        let cx = Context::new().with_baggage(state.baggage());
        let inherited: BTreeMap<String, String> = inherited_attributes(&cx)
            .into_iter()
            .map(|kv| (kv.key.to_string(), kv.value.to_string()))
            .collect();
        assert_eq!(attributes_of(&state), inherited);
    }
}
