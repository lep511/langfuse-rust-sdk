//! Configuration of [`LangfuseTracer`](super::LangfuseTracer).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use opentelemetry_sdk::trace::SpanData;

use crate::error::LangfuseError;

/// Transform applied to observation input, output and metadata before they
/// are recorded, e.g. to redact personal data.
pub type MaskFn = Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>;

/// Decides whether a finished span is exported to Langfuse.
pub type ExportFilter = Arc<dyn Fn(&SpanData) -> bool + Send + Sync>;

/// Environment variable with the tracing environment (e.g. `production`).
pub const ENV_TRACING_ENVIRONMENT: &str = "LANGFUSE_TRACING_ENVIRONMENT";
/// Environment variable with the release of the application.
pub const ENV_RELEASE: &str = "LANGFUSE_RELEASE";
/// Environment variable with the share of traces to keep (`0.0`–`1.0`).
pub const ENV_SAMPLE_RATE: &str = "LANGFUSE_SAMPLE_RATE";
/// Environment variable that disables tracing when `false` or `0`.
pub const ENV_TRACING_ENABLED: &str = "LANGFUSE_TRACING_ENABLED";
/// Environment variable with the number of spans that triggers an export.
pub const ENV_FLUSH_AT: &str = "LANGFUSE_FLUSH_AT";
/// Environment variable with the export interval in seconds.
pub const ENV_FLUSH_INTERVAL: &str = "LANGFUSE_FLUSH_INTERVAL";
/// Environment variable with the maximum size of one export request in bytes.
pub const ENV_MAX_BATCH_SIZE_BYTES: &str = "LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES";

/// Options of [`LangfuseTracer::new`](super::LangfuseTracer::new); every field
/// has a default.
#[derive(Clone)]
pub struct TracingOptions {
    /// Environment of every observation unless a propagated one applies
    /// (default: none, which Langfuse shows as `default`).
    pub environment: Option<String>,
    /// Release of the application, recorded on every observation.
    pub release: Option<String>,
    /// Share of traces to record, decided per trace ID (default: `1.0`).
    pub sample_rate: f64,
    /// `false` turns every observation into a no-op and exports nothing
    /// (default: `true`).
    pub enabled: bool,
    /// Export as soon as this many spans are queued (default: 512).
    pub flush_at: usize,
    /// Export queued spans at least this often (default: 5 s).
    pub flush_interval: Duration,
    /// Spans kept in memory before new ones are dropped (default: 2048).
    pub max_queue_size: usize,
    /// Split exports so one request stays under this many JSON bytes
    /// (default: 1 MiB). A single larger span is still sent on its own.
    pub max_batch_bytes: usize,
    /// Applied to input, output and metadata before they are recorded.
    pub mask: Option<MaskFn>,
    /// Which finished spans to export (default:
    /// [`is_default_export_span`](super::is_default_export_span): Langfuse
    /// observations, `gen_ai.*` spans and known LLM instrumentations).
    pub should_export_span: Option<ExportFilter>,
}

impl Default for TracingOptions {
    fn default() -> Self {
        Self {
            environment: None,
            release: None,
            sample_rate: 1.0,
            enabled: true,
            flush_at: 512,
            flush_interval: Duration::from_secs(5),
            max_queue_size: 2048,
            max_batch_bytes: 1024 * 1024,
            mask: None,
            should_export_span: None,
        }
    }
}

impl fmt::Debug for TracingOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TracingOptions")
            .field("environment", &self.environment)
            .field("release", &self.release)
            .field("sample_rate", &self.sample_rate)
            .field("enabled", &self.enabled)
            .field("flush_at", &self.flush_at)
            .field("flush_interval", &self.flush_interval)
            .field("max_queue_size", &self.max_queue_size)
            .field("max_batch_bytes", &self.max_batch_bytes)
            .field("mask", &self.mask.as_ref().map(|_| "<fn>"))
            .field(
                "should_export_span",
                &self.should_export_span.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

impl TracingOptions {
    /// Defaults overridden by the `LANGFUSE_*` tracing variables:
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `LANGFUSE_TRACING_ENVIRONMENT` | `environment` |
    /// | `LANGFUSE_RELEASE` | `release` |
    /// | `LANGFUSE_SAMPLE_RATE` | `sample_rate` |
    /// | `LANGFUSE_TRACING_ENABLED` (`false`/`0` disable) | `enabled` |
    /// | `LANGFUSE_FLUSH_AT` | `flush_at` |
    /// | `LANGFUSE_FLUSH_INTERVAL` (seconds) | `flush_interval` |
    /// | `LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES` | `max_batch_bytes` |
    ///
    /// Empty values count as unset; invalid numbers return
    /// [`LangfuseError::Config`].
    pub fn from_env() -> Result<Self, LangfuseError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, LangfuseError> {
        let var = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let invalid = |name: &str, raw: &str, expected: &str| {
            LangfuseError::Config(format!("Invalid {name} '{raw}': expected {expected}"))
        };
        let mut options = Self {
            environment: var(ENV_TRACING_ENVIRONMENT),
            release: var(ENV_RELEASE),
            ..Self::default()
        };
        if let Some(raw) = var(ENV_SAMPLE_RATE) {
            options.sample_rate = raw
                .parse::<f64>()
                .ok()
                .filter(|rate| (0.0..=1.0).contains(rate))
                .ok_or_else(|| invalid(ENV_SAMPLE_RATE, &raw, "a number between 0 and 1"))?;
        }
        if let Some(raw) = var(ENV_TRACING_ENABLED) {
            options.enabled = !matches!(raw.to_ascii_lowercase().as_str(), "false" | "0");
        }
        if let Some(raw) = var(ENV_FLUSH_AT) {
            options.flush_at = raw
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid(ENV_FLUSH_AT, &raw, "a positive integer"))?;
        }
        if let Some(raw) = var(ENV_FLUSH_INTERVAL) {
            options.flush_interval = raw
                .parse::<f64>()
                .ok()
                .filter(|secs| *secs > 0.0)
                .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
                .ok_or_else(|| {
                    invalid(
                        ENV_FLUSH_INTERVAL,
                        &raw,
                        "a number of seconds greater than 0",
                    )
                })?;
        }
        if let Some(raw) = var(ENV_MAX_BATCH_SIZE_BYTES) {
            options.max_batch_bytes = raw
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid(ENV_MAX_BATCH_SIZE_BYTES, &raw, "a positive integer"))?;
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn from_vars(vars: &[(&str, &str)]) -> Result<TracingOptions, LangfuseError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        TracingOptions::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn defaults_without_variables() {
        let options = from_vars(&[]).unwrap();
        assert_eq!(options.sample_rate, 1.0);
        assert!(options.enabled);
        assert_eq!(options.flush_at, 512);
        assert_eq!(options.flush_interval, Duration::from_secs(5));
        assert!(options.environment.is_none());
    }

    #[test]
    fn reads_every_variable() {
        let options = from_vars(&[
            ("LANGFUSE_TRACING_ENVIRONMENT", "staging"),
            ("LANGFUSE_RELEASE", "v1.2"),
            ("LANGFUSE_SAMPLE_RATE", "0.25"),
            ("LANGFUSE_TRACING_ENABLED", "False"),
            ("LANGFUSE_FLUSH_AT", "10"),
            ("LANGFUSE_FLUSH_INTERVAL", "0.5"),
            ("LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES", "2048"),
        ])
        .unwrap();
        assert_eq!(options.environment.as_deref(), Some("staging"));
        assert_eq!(options.release.as_deref(), Some("v1.2"));
        assert_eq!(options.sample_rate, 0.25);
        assert!(!options.enabled);
        assert_eq!(options.flush_at, 10);
        assert_eq!(options.flush_interval, Duration::from_millis(500));
        assert_eq!(options.max_batch_bytes, 2048);
    }

    #[test]
    fn rejects_invalid_numbers() {
        for (name, raw) in [
            ("LANGFUSE_SAMPLE_RATE", "1.5"),
            ("LANGFUSE_SAMPLE_RATE", "x"),
            ("LANGFUSE_FLUSH_AT", "0"),
            ("LANGFUSE_FLUSH_INTERVAL", "-1"),
            ("LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES", "big"),
        ] {
            match from_vars(&[(name, raw)]) {
                Err(LangfuseError::Config(msg)) => assert!(msg.contains(name), "{msg}"),
                other => panic!("{name}={raw}: expected Config error, got {other:?}"),
            }
        }
    }
}
