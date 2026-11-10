//! Configuration of [`LangfuseTracer`](super::LangfuseTracer).

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use opentelemetry_sdk::trace::{IdGenerator, SpanData, SpanExporter};

use super::exporter::{MaskableSpan, SharedSpanExporter};
use crate::error::LangfuseError;
use crate::skills::SkillReference;

/// Transform applied to observation input, output and metadata before they
/// are recorded, e.g. to redact personal data.
pub type MaskFn = Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>;

/// Decides whether a finished span is exported to Langfuse.
pub type ExportFilter = Arc<dyn Fn(&SpanData) -> bool + Send + Sync>;

/// Hook run on every export batch before it is sent; see
/// [`TracingOptions::mask_otel_spans`].
pub type SpanMaskFn = Arc<dyn Fn(&mut [MaskableSpan]) + Send + Sync>;

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
/// Environment variable that disables media upload when `false` or `0`.
pub const ENV_MEDIA_UPLOAD_ENABLED: &str = "LANGFUSE_MEDIA_UPLOAD_ENABLED";
/// Environment variable with the compression of span exports (`gzip` or `none`).
pub const ENV_OTEL_COMPRESSION: &str = "LANGFUSE_OTEL_COMPRESSION";
/// Environment variable with the path spans are exported to, relative to the
/// base URL.
pub const ENV_OTEL_TRACES_EXPORT_PATH: &str = "LANGFUSE_OTEL_TRACES_EXPORT_PATH";

/// Standard OpenTelemetry variables read by [`TracingOptions::from_env`].
const OTEL_EXPORTER_OTLP_TRACES_COMPRESSION: &str = "OTEL_EXPORTER_OTLP_TRACES_COMPRESSION";
const OTEL_EXPORTER_OTLP_COMPRESSION: &str = "OTEL_EXPORTER_OTLP_COMPRESSION";
const OTEL_SDK_DISABLED: &str = "OTEL_SDK_DISABLED";

/// Variables holding the commit of common CI and hosting platforms, tried in
/// this order for the release when `LANGFUSE_RELEASE` is unset.
const CI_RELEASE_VARIABLES: &[&str] = &[
    "RENDER_GIT_COMMIT",   // Render
    "CI_COMMIT_SHA",       // GitLab CI
    "CIRCLE_SHA1",         // CircleCI
    "SOURCE_VERSION",      // Heroku
    "TRAVIS_COMMIT",       // Travis CI
    "GIT_COMMIT",          // Jenkins
    "GITHUB_SHA",          // GitHub Actions
    "BITBUCKET_COMMIT",    // Bitbucket Pipelines
    "BUILD_SOURCEVERSION", // Azure Pipelines
    "DRONE_COMMIT_SHA",    // Drone CI
];

/// Compression of the requests that export spans to Langfuse.
///
/// Parsed case-insensitively from `gzip` and `none`, the values of
/// `LANGFUSE_OTEL_COMPRESSION` and the OpenTelemetry variables. The Langfuse
/// endpoint only decodes gzip, so `deflate` is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Compression {
    /// `Content-Encoding: gzip` (default).
    #[default]
    Gzip,
    /// Uncompressed JSON.
    None,
}

impl Compression {
    /// The configuration value: `gzip` or `none`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::None => "none",
        }
    }
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Compression {
    type Err = LangfuseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gzip" => Ok(Self::Gzip),
            "none" => Ok(Self::None),
            _ => Err(LangfuseError::Config(format!(
                "Invalid compression '{value}': expected 'gzip' or 'none'"
            ))),
        }
    }
}

/// Options of [`LangfuseTracer::new`](super::LangfuseTracer::new); every field
/// has a default.
///
/// Build it from [`TracingOptions::default`] (or [`from_env`](Self::from_env))
/// with the `with_*` methods; new options can then be added without breaking
/// your code. The fields stay public for reading and assignment.
///
/// ```
/// use langfuse_rust_sdk::otel::TracingOptions;
/// use std::time::Duration;
///
/// let options = TracingOptions::default()
///     .with_environment("production")
///     .with_sample_rate(0.25)
///     .with_flush_interval(Duration::from_secs(2));
/// assert_eq!(options.sample_rate, 0.25);
/// ```
#[derive(Clone)]
#[non_exhaustive]
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
    /// Upload base64 media found in exported span attributes and replace it
    /// with its reference string (default: `true`); see [`crate::media`].
    pub media_upload: bool,
    /// Skills available to the application (e.g. from
    /// [`discover_skills`](crate::skills::discover_skills)), recorded on the
    /// spans started inside scopes that enable
    /// [`attach_skills`](super::PropagatedAttributes::attach_skills); never on
    /// spans of another Langfuse project. Serialized once, when the tracer is
    /// created.
    pub skills: Option<Vec<SkillReference>>,
    /// Compression of the export requests (default: [`Compression::Gzip`]).
    pub compression: Compression,
    /// Path spans are exported to, relative to the client's base URL (default:
    /// `api/public/otel/v1/traces`); for proxies that expose the OTLP
    /// endpoint elsewhere. Leading `/` are ignored, so a base path prefix is
    /// kept; an empty path means the default.
    pub traces_export_path: Option<String>,
    /// Called with every export batch, after
    /// [`should_export_span`](Self::should_export_span) and media upload and
    /// before the spans are sent, to redact or add attributes; for input,
    /// output and metadata, [`mask`](Self::mask) is simpler. Only attributes
    /// can change (see [`MaskableSpan`]). A batch is what one export sends
    /// (see [`flush_at`](Self::flush_at)), not necessarily a whole trace. If
    /// the hook panics, the whole batch is dropped (logged as an error), so
    /// unmasked data never leaves the process.
    pub mask_otel_spans: Option<SpanMaskFn>,
    /// Generates the trace and span IDs of the tracer's provider (default:
    /// random). Ignored by
    /// [`LangfuseTracer::with_provider`](super::LangfuseTracer::with_provider):
    /// configure the ID generator on that provider instead.
    pub id_generator: Option<Arc<dyn IdGenerator>>,
    /// Exporter that receives the spans instead of the Langfuse OTLP endpoint,
    /// e.g. an OTLP exporter to a collector. Media upload and
    /// [`mask_otel_spans`](Self::mask_otel_spans) still apply, and batches
    /// still follow [`flush_at`](Self::flush_at) and
    /// [`flush_interval`](Self::flush_interval). The client's base URL,
    /// credentials and retries, [`compression`](Self::compression),
    /// [`traces_export_path`](Self::traces_export_path) and
    /// [`max_batch_bytes`](Self::max_batch_bytes) do not: configure the
    /// exporter itself. When it sends to Langfuse, add the header
    /// `x-langfuse-ingestion-version: 4`.
    pub span_exporter: Option<SharedSpanExporter>,
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
            media_upload: true,
            skills: None,
            compression: Compression::Gzip,
            traces_export_path: None,
            mask_otel_spans: None,
            id_generator: None,
            span_exporter: None,
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
            .field("media_upload", &self.media_upload)
            .field("skills", &self.skills)
            .field("compression", &self.compression)
            .field("traces_export_path", &self.traces_export_path)
            .field(
                "mask_otel_spans",
                &self.mask_otel_spans.as_ref().map(|_| "<fn>"),
            )
            .field("id_generator", &self.id_generator)
            .field("span_exporter", &self.span_exporter)
            .finish()
    }
}

impl TracingOptions {
    /// Set [`environment`](Self::environment).
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Set [`release`](Self::release).
    pub fn with_release(mut self, release: impl Into<String>) -> Self {
        self.release = Some(release.into());
        self
    }

    /// Set [`sample_rate`](Self::sample_rate).
    pub fn with_sample_rate(mut self, sample_rate: f64) -> Self {
        self.sample_rate = sample_rate;
        self
    }

    /// Set [`enabled`](Self::enabled).
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set [`flush_at`](Self::flush_at).
    pub fn with_flush_at(mut self, flush_at: usize) -> Self {
        self.flush_at = flush_at;
        self
    }

    /// Set [`flush_interval`](Self::flush_interval).
    pub fn with_flush_interval(mut self, flush_interval: Duration) -> Self {
        self.flush_interval = flush_interval;
        self
    }

    /// Set [`max_queue_size`](Self::max_queue_size).
    pub fn with_max_queue_size(mut self, max_queue_size: usize) -> Self {
        self.max_queue_size = max_queue_size;
        self
    }

    /// Set [`max_batch_bytes`](Self::max_batch_bytes).
    pub fn with_max_batch_bytes(mut self, max_batch_bytes: usize) -> Self {
        self.max_batch_bytes = max_batch_bytes;
        self
    }

    /// Set [`mask`](Self::mask).
    pub fn with_mask<F>(mut self, mask: F) -> Self
    where
        F: Fn(serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
    {
        self.mask = Some(Arc::new(mask));
        self
    }

    /// Set [`should_export_span`](Self::should_export_span).
    pub fn with_should_export_span<F>(mut self, filter: F) -> Self
    where
        F: Fn(&SpanData) -> bool + Send + Sync + 'static,
    {
        self.should_export_span = Some(Arc::new(filter));
        self
    }

    /// Set [`media_upload`](Self::media_upload).
    pub fn with_media_upload(mut self, media_upload: bool) -> Self {
        self.media_upload = media_upload;
        self
    }

    /// Set [`skills`](Self::skills).
    pub fn with_skills<I>(mut self, skills: I) -> Self
    where
        I: IntoIterator<Item = SkillReference>,
    {
        self.skills = Some(skills.into_iter().collect());
        self
    }

    /// Set [`compression`](Self::compression).
    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Set [`traces_export_path`](Self::traces_export_path).
    pub fn with_traces_export_path(mut self, path: impl Into<String>) -> Self {
        self.traces_export_path = Some(path.into());
        self
    }

    /// Set [`mask_otel_spans`](Self::mask_otel_spans).
    ///
    /// ```
    /// use langfuse_rust_sdk::otel::TracingOptions;
    /// use langfuse_rust_sdk::otel::opentelemetry::KeyValue;
    ///
    /// let options = TracingOptions::default().with_mask_otel_spans(|spans| {
    ///     for span in spans {
    ///         if span.remove_attribute("http.request.header.authorization").is_some() {
    ///             span.set_attribute(KeyValue::new("security.redacted", true));
    ///         }
    ///     }
    /// });
    /// assert!(options.mask_otel_spans.is_some());
    /// ```
    pub fn with_mask_otel_spans<F>(mut self, mask: F) -> Self
    where
        F: Fn(&mut [MaskableSpan]) + Send + Sync + 'static,
    {
        self.mask_otel_spans = Some(Arc::new(mask));
        self
    }

    /// Set [`id_generator`](Self::id_generator).
    pub fn with_id_generator<G>(mut self, id_generator: G) -> Self
    where
        G: IdGenerator + 'static,
    {
        self.id_generator = Some(Arc::new(id_generator));
        self
    }

    /// Set [`span_exporter`](Self::span_exporter). Clones of these options
    /// share the exporter.
    pub fn with_span_exporter<E>(mut self, exporter: E) -> Self
    where
        E: SpanExporter + 'static,
    {
        self.span_exporter = Some(SharedSpanExporter::new(exporter));
        self
    }

    /// Defaults overridden by the `LANGFUSE_*` tracing variables:
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `LANGFUSE_TRACING_ENVIRONMENT` | `environment` |
    /// | `LANGFUSE_RELEASE`, else the commit of a CI platform (below) | `release` |
    /// | `LANGFUSE_SAMPLE_RATE` | `sample_rate` |
    /// | `LANGFUSE_TRACING_ENABLED` (`false`/`0` disable) | `enabled` |
    /// | `OTEL_SDK_DISABLED` (`true` disables) | `enabled` |
    /// | `LANGFUSE_FLUSH_AT` | `flush_at` |
    /// | `LANGFUSE_FLUSH_INTERVAL` (seconds) | `flush_interval` |
    /// | `LANGFUSE_OTEL_MAX_BATCH_SIZE_BYTES` | `max_batch_bytes` |
    /// | `LANGFUSE_MEDIA_UPLOAD_ENABLED` (`false`/`0` disable) | `media_upload` |
    /// | `LANGFUSE_OTEL_COMPRESSION`, `OTEL_EXPORTER_OTLP_TRACES_COMPRESSION`, `OTEL_EXPORTER_OTLP_COMPRESSION` (`gzip`/`none`) | `compression` |
    /// | `LANGFUSE_OTEL_TRACES_EXPORT_PATH` | `traces_export_path` |
    ///
    /// Without `LANGFUSE_RELEASE`, the release is the first of
    /// `RENDER_GIT_COMMIT`, `CI_COMMIT_SHA`, `CIRCLE_SHA1`, `SOURCE_VERSION`,
    /// `TRAVIS_COMMIT`, `GIT_COMMIT`, `GITHUB_SHA`, `BITBUCKET_COMMIT`,
    /// `BUILD_SOURCEVERSION` and `DRONE_COMMIT_SHA` that is set, as in the
    /// Python SDK. The compression variables are tried in the order listed: an
    /// invalid value falls through to the next one (with a warning for
    /// `LANGFUSE_OTEL_COMPRESSION`), then gzip. `OTEL_SDK_DISABLED=true` turns
    /// tracing off, as the OpenTelemetry SDK under the Python SDK does (the
    /// Rust `opentelemetry_sdk` crate ignores the variable).
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
            release: var(ENV_RELEASE)
                .or_else(|| CI_RELEASE_VARIABLES.iter().find_map(|name| var(name))),
            traces_export_path: var(ENV_OTEL_TRACES_EXPORT_PATH),
            compression: resolve_compression(&var),
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
        if otel_sdk_disabled(&var) {
            options.enabled = false;
        }
        if let Some(raw) = var(ENV_MEDIA_UPLOAD_ENABLED) {
            options.media_upload = !matches!(raw.to_ascii_lowercase().as_str(), "false" | "0");
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

/// The first valid compression of `LANGFUSE_OTEL_COMPRESSION`,
/// `OTEL_EXPORTER_OTLP_TRACES_COMPRESSION` and `OTEL_EXPORTER_OTLP_COMPRESSION`,
/// else gzip. Like the Python SDK, only an invalid Langfuse value is reported.
fn resolve_compression(var: &impl Fn(&str) -> Option<String>) -> Compression {
    let candidates = [
        (ENV_OTEL_COMPRESSION, true),
        (OTEL_EXPORTER_OTLP_TRACES_COMPRESSION, false),
        (OTEL_EXPORTER_OTLP_COMPRESSION, false),
    ];
    for (name, warn) in candidates {
        let Some(raw) = var(name) else { continue };
        match raw.parse() {
            Ok(compression) => return compression,
            Err(_) if warn => tracing::warn!(
                "Invalid {name}={raw:?}: expected 'gzip' or 'none'; falling back to the next compression setting, then gzip"
            ),
            Err(_) => {}
        }
    }
    Compression::Gzip
}

/// Whether `OTEL_SDK_DISABLED` is `true` (case-insensitive).
fn otel_sdk_disabled(var: &impl Fn(&str) -> Option<String>) -> bool {
    var(OTEL_SDK_DISABLED).is_some_and(|raw| raw.trim().eq_ignore_ascii_case("true"))
}

/// The warning to log when a tracer is created while `OTEL_SDK_DISABLED` is
/// `true`; `enabled` tells whether its options still enable tracing.
pub(crate) fn otel_sdk_disabled_warning(
    lookup: impl Fn(&str) -> Option<String>,
    enabled: bool,
) -> Option<&'static str> {
    if !otel_sdk_disabled(&lookup) {
        return None;
    }
    Some(if enabled {
        "OTEL_SDK_DISABLED is set, but these TracingOptions enable tracing, so traces are still exported (TracingOptions::from_env disables tracing when it is set)."
    } else {
        "OTEL_SDK_DISABLED is set. Langfuse tracing is disabled and no traces will appear in the UI."
    })
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
            ("LANGFUSE_MEDIA_UPLOAD_ENABLED", "0"),
        ])
        .unwrap();
        assert!(!options.media_upload);
        assert_eq!(options.environment.as_deref(), Some("staging"));
        assert_eq!(options.release.as_deref(), Some("v1.2"));
        assert_eq!(options.sample_rate, 0.25);
        assert!(!options.enabled);
        assert_eq!(options.flush_at, 10);
        assert_eq!(options.flush_interval, Duration::from_millis(500));
        assert_eq!(options.max_batch_bytes, 2048);
    }

    #[test]
    fn compression_defaults_to_gzip_and_follows_the_variables_in_order() {
        assert_eq!(from_vars(&[]).unwrap().compression, Compression::Gzip);
        let cases: &[(&[(&str, &str)], Compression)] = &[
            (
                &[("LANGFUSE_OTEL_COMPRESSION", " NONE ")],
                Compression::None,
            ),
            (
                &[("OTEL_EXPORTER_OTLP_TRACES_COMPRESSION", "none")],
                Compression::None,
            ),
            (
                &[("OTEL_EXPORTER_OTLP_COMPRESSION", "none")],
                Compression::None,
            ),
            (
                &[
                    ("LANGFUSE_OTEL_COMPRESSION", "gzip"),
                    ("OTEL_EXPORTER_OTLP_TRACES_COMPRESSION", "none"),
                ],
                Compression::Gzip,
            ),
            (
                &[
                    ("OTEL_EXPORTER_OTLP_TRACES_COMPRESSION", "gzip"),
                    ("OTEL_EXPORTER_OTLP_COMPRESSION", "none"),
                ],
                Compression::Gzip,
            ),
            // Invalid values fall through to the next setting, then gzip.
            (
                &[
                    ("LANGFUSE_OTEL_COMPRESSION", "deflate"),
                    ("OTEL_EXPORTER_OTLP_COMPRESSION", "none"),
                ],
                Compression::None,
            ),
            (
                &[("OTEL_EXPORTER_OTLP_TRACES_COMPRESSION", "zstd")],
                Compression::Gzip,
            ),
            (&[("LANGFUSE_OTEL_COMPRESSION", "")], Compression::Gzip),
        ];
        for (vars, expected) in cases {
            assert_eq!(from_vars(vars).unwrap().compression, *expected, "{vars:?}");
        }
    }

    #[test]
    fn compression_parses_and_displays() {
        assert_eq!("GZip".parse::<Compression>().unwrap(), Compression::Gzip);
        assert_eq!("none".parse::<Compression>().unwrap(), Compression::None);
        assert!(matches!(
            "deflate".parse::<Compression>(),
            Err(LangfuseError::Config(_))
        ));
        assert_eq!(Compression::None.to_string(), "none");
        assert_eq!(Compression::default(), Compression::Gzip);
    }

    #[test]
    fn release_falls_back_to_ci_commit_variables_in_order() {
        assert_eq!(from_vars(&[]).unwrap().release, None);
        let options = from_vars(&[("GITHUB_SHA", "gh"), ("CIRCLE_SHA1", "circle")]).unwrap();
        assert_eq!(options.release.as_deref(), Some("circle"));
        let options =
            from_vars(&[("RENDER_GIT_COMMIT", ""), ("DRONE_COMMIT_SHA", "drone")]).unwrap();
        assert_eq!(options.release.as_deref(), Some("drone"));
        let options =
            from_vars(&[("LANGFUSE_RELEASE", "v2"), ("RENDER_GIT_COMMIT", "abc")]).unwrap();
        assert_eq!(options.release.as_deref(), Some("v2"));
        for (i, name) in CI_RELEASE_VARIABLES.iter().enumerate() {
            let later: Vec<(&str, &str)> = CI_RELEASE_VARIABLES[i..]
                .iter()
                .map(|n| (*n, if n == name { "first" } else { "later" }))
                .collect();
            assert_eq!(
                from_vars(&later).unwrap().release.as_deref(),
                Some("first"),
                "{name}"
            );
        }
    }

    #[test]
    fn otel_sdk_disabled_turns_tracing_off() {
        assert!(!from_vars(&[("OTEL_SDK_DISABLED", "TRUE")]).unwrap().enabled);
        assert!(
            !from_vars(&[
                ("OTEL_SDK_DISABLED", "true"),
                ("LANGFUSE_TRACING_ENABLED", "true")
            ])
            .unwrap()
            .enabled
        );
        assert!(
            from_vars(&[("OTEL_SDK_DISABLED", "false")])
                .unwrap()
                .enabled
        );
        assert!(from_vars(&[("OTEL_SDK_DISABLED", "1")]).unwrap().enabled);
    }

    #[test]
    fn otel_sdk_disabled_warning_depends_on_the_options() {
        let set = |name: &str| (name == "OTEL_SDK_DISABLED").then(|| "True".to_owned());
        let unset = |_: &str| None;
        assert_eq!(otel_sdk_disabled_warning(unset, true), None);
        assert_eq!(otel_sdk_disabled_warning(unset, false), None);
        assert!(
            otel_sdk_disabled_warning(set, false)
                .unwrap()
                .contains("no traces will appear")
        );
        assert!(
            otel_sdk_disabled_warning(set, true)
                .unwrap()
                .contains("still exported")
        );
    }

    #[test]
    fn traces_export_path_is_read() {
        let options =
            from_vars(&[("LANGFUSE_OTEL_TRACES_EXPORT_PATH", "/proxy/v1/traces")]).unwrap();
        assert_eq!(
            options.traces_export_path.as_deref(),
            Some("/proxy/v1/traces")
        );
        assert_eq!(from_vars(&[]).unwrap().traces_export_path, None);
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
