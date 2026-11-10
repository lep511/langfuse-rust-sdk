//! SDK configuration and builder.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::error::LangfuseError;
use crate::retry::RetryPolicy;

const DEFAULT_BASE_URL: &str = "https://cloud.langfuse.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_PROMPT_CACHE_TTL: Duration = Duration::from_secs(60);
const SDK_NAME: &str = "rust";
const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Environment variable holding the public API key (required by [`ConfigBuilder::from_env`]).
pub const ENV_PUBLIC_KEY: &str = "LANGFUSE_PUBLIC_KEY";
/// Environment variable holding the secret API key (required by [`ConfigBuilder::from_env`]).
pub const ENV_SECRET_KEY: &str = "LANGFUSE_SECRET_KEY";
/// Environment variable holding the API base URL.
pub const ENV_BASE_URL: &str = "LANGFUSE_BASE_URL";
/// Deprecated alias of [`ENV_BASE_URL`], read only when `LANGFUSE_BASE_URL` is unset.
pub const ENV_HOST: &str = "LANGFUSE_HOST";
/// Environment variable holding the request timeout in seconds (e.g. `10` or `2.5`).
pub const ENV_TIMEOUT: &str = "LANGFUSE_TIMEOUT";
/// Environment variable holding the default prompt cache TTL in seconds (`0` disables caching).
pub const ENV_PROMPT_CACHE_TTL: &str = "LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS";
/// Environment variable with the number of queued scores that triggers a send.
pub const ENV_FLUSH_AT: &str = "LANGFUSE_FLUSH_AT";
/// Environment variable with the send interval of queued scores, in seconds.
pub const ENV_FLUSH_INTERVAL: &str = "LANGFUSE_FLUSH_INTERVAL";
/// Environment variable with the maximum body size of one score batch, in bytes.
pub const ENV_MAX_BATCH_SIZE_BYTES: &str = "LANGFUSE_MAX_BATCH_SIZE_BYTES";
/// Environment variable with the number of media uploads run at the same time
/// (the Python SDK's upload thread count).
pub const ENV_MEDIA_UPLOAD_THREAD_COUNT: &str = "LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT";

/// Body size limit of `POST /api/public/scores`.
const SCORES_BODY_LIMIT_BYTES: usize = 1_000_000;

/// Batching of [`LangfuseClient::score`](crate::LangfuseClient::score).
///
/// Build it from [`ScoreQueueOptions::default`] with the `with_*` methods, so
/// new options can be added without breaking your code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScoreQueueOptions {
    /// Send as soon as this many scores are queued (default: 15).
    pub flush_at: usize,
    /// Send queued scores at least this often (default: 1 s).
    pub flush_interval: Duration,
    /// Keep each request body under this many bytes (default and maximum:
    /// 1,000,000, the limit of the endpoint). Larger scores are dropped.
    pub max_batch_bytes: usize,
    /// Scores kept in memory before new ones are dropped (default: 100,000).
    pub max_queue_size: usize,
}

impl ScoreQueueOptions {
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

    /// Set [`max_batch_bytes`](Self::max_batch_bytes).
    pub fn with_max_batch_bytes(mut self, max_batch_bytes: usize) -> Self {
        self.max_batch_bytes = max_batch_bytes;
        self
    }

    /// Set [`max_queue_size`](Self::max_queue_size).
    pub fn with_max_queue_size(mut self, max_queue_size: usize) -> Self {
        self.max_queue_size = max_queue_size;
        self
    }
}

impl Default for ScoreQueueOptions {
    fn default() -> Self {
        Self {
            flush_at: 15,
            flush_interval: Duration::from_secs(1),
            max_batch_bytes: SCORES_BODY_LIMIT_BYTES,
            max_queue_size: 100_000,
        }
    }
}

/// Resolved configuration consumed by [`LangfuseClient::new`](crate::LangfuseClient::new).
///
/// Construct one via [`ClientConfig::builder`] or [`ClientConfig::from_env`].
#[derive(Debug)]
pub struct ClientConfig {
    pub(crate) base_url: Url,
    pub(crate) auth_header: HeaderValue,
    pub(crate) public_key: String,
    pub(crate) default_headers: HeaderMap,
    /// `None` when [`ConfigBuilder::timeout`] was never called.
    pub(crate) timeout: Option<Duration>,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) http_client: Option<reqwest::Client>,
    pub(crate) prompt_cache_ttl: Duration,
    pub(crate) score_queue: ScoreQueueOptions,
    pub(crate) media_upload_concurrency: usize,
}

impl ClientConfig {
    /// Start building a configuration with the required API keys.
    pub fn builder(public_key: impl Into<String>, secret_key: impl Into<String>) -> ConfigBuilder {
        ConfigBuilder {
            public_key: public_key.into(),
            secret_key: secret_key.into(),
            base_url: None,
            timeout: None,
            retry_policy: None,
            extra_headers: HeaderMap::new(),
            header_error: None,
            http_client: None,
            prompt_cache_ttl: None,
            score_queue: None,
            media_upload_concurrency: None,
        }
    }

    /// Build a configuration from `LANGFUSE_*` environment variables.
    ///
    /// Shorthand for `ConfigBuilder::from_env()?.build()`; see
    /// [`ConfigBuilder::from_env`] for the variables read.
    pub fn from_env() -> Result<Self, LangfuseError> {
        ConfigBuilder::from_env()?.build()
    }

    /// The resolved base URL of the Langfuse API.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// The HTTP request timeout (default: 30 s).
    ///
    /// With a custom [`http_client`](ConfigBuilder::http_client) and no explicit
    /// [`timeout`](ConfigBuilder::timeout), the custom client's own timeout
    /// applies instead of this value.
    pub fn timeout(&self) -> Duration {
        self.timeout.unwrap_or(DEFAULT_TIMEOUT)
    }

    /// Default time-to-live of cached prompts (default: 60 s; zero disables caching).
    pub fn prompt_cache_ttl(&self) -> Duration {
        self.prompt_cache_ttl
    }

    /// Batching of queued scores.
    pub fn score_queue(&self) -> &ScoreQueueOptions {
        &self.score_queue
    }

    /// Media uploads run at the same time by the background upload queue
    /// (default: 1).
    pub fn media_upload_concurrency(&self) -> usize {
        self.media_upload_concurrency
    }
}

/// Builder for [`ClientConfig`].
///
/// Created via [`ClientConfig::builder`] or [`ConfigBuilder::from_env`].
pub struct ConfigBuilder {
    public_key: String,
    secret_key: String,
    base_url: Option<String>,
    timeout: Option<Duration>,
    retry_policy: Option<RetryPolicy>,
    extra_headers: HeaderMap,
    /// First invalid [`header`](Self::header) call, reported by [`build`](Self::build).
    header_error: Option<String>,
    http_client: Option<reqwest::Client>,
    prompt_cache_ttl: Option<Duration>,
    score_queue: Option<ScoreQueueOptions>,
    media_upload_concurrency: Option<usize>,
}

impl ConfigBuilder {
    /// Start a builder from `LANGFUSE_*` environment variables, so further
    /// settings can still be overridden before [`build`](Self::build).
    ///
    /// | Variable | Use |
    /// |---|---|
    /// | `LANGFUSE_PUBLIC_KEY` | Public API key (required) |
    /// | `LANGFUSE_SECRET_KEY` | Secret API key (required) |
    /// | `LANGFUSE_BASE_URL` | API base URL (default: `https://cloud.langfuse.com`) |
    /// | `LANGFUSE_HOST` | Deprecated alias of `LANGFUSE_BASE_URL`, used only when that is unset |
    /// | `LANGFUSE_TIMEOUT` | Request timeout in seconds, e.g. `10` or `2.5` (default: 30) |
    /// | `LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS` | Default prompt cache TTL in seconds; `0` disables caching (default: 60) |
    /// | `LANGFUSE_FLUSH_AT` | Queued scores that trigger a send (default: 15) |
    /// | `LANGFUSE_FLUSH_INTERVAL` | Score send interval in seconds (default: 1) |
    /// | `LANGFUSE_MAX_BATCH_SIZE_BYTES` | Maximum score batch body in bytes (default and maximum: 1,000,000) |
    /// | `LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT` | Media uploads run at the same time (default: 1) |
    ///
    /// Empty values count as unset. Missing keys and invalid numbers return
    /// [`LangfuseError::Config`]. The variables are
    /// read from the process environment; load a `.env` file first (e.g. with
    /// `dotenvy`) if needed.
    pub fn from_env() -> Result<Self, LangfuseError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// [`from_env`](Self::from_env) with an injectable variable lookup, for tests.
    pub(crate) fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, LangfuseError> {
        let var = |name: &str| lookup(name).filter(|v| !v.trim().is_empty());
        let required = |name: &str| {
            var(name).ok_or_else(|| LangfuseError::Config(format!("{name} is not set")))
        };

        let mut builder =
            ClientConfig::builder(required(ENV_PUBLIC_KEY)?, required(ENV_SECRET_KEY)?);
        if let Some(url) = var(ENV_BASE_URL).or_else(|| var(ENV_HOST)) {
            builder = builder.base_url(url);
        }
        if let Some(raw) = var(ENV_TIMEOUT) {
            builder = builder.timeout(parse_secs(ENV_TIMEOUT, &raw, false)?);
        }
        if let Some(raw) = var(ENV_PROMPT_CACHE_TTL) {
            builder = builder.prompt_cache_ttl(parse_secs(ENV_PROMPT_CACHE_TTL, &raw, true)?);
        }
        let mut queue = ScoreQueueOptions::default();
        if let Some(raw) = var(ENV_FLUSH_AT) {
            queue.flush_at = parse_count(ENV_FLUSH_AT, &raw)?;
        }
        if let Some(raw) = var(ENV_FLUSH_INTERVAL) {
            queue.flush_interval = parse_secs(ENV_FLUSH_INTERVAL, &raw, false)?;
        }
        if let Some(raw) = var(ENV_MAX_BATCH_SIZE_BYTES) {
            queue.max_batch_bytes = parse_count(ENV_MAX_BATCH_SIZE_BYTES, &raw)?;
        }
        if queue != ScoreQueueOptions::default() {
            builder = builder.score_queue(queue);
        }
        if let Some(raw) = var(ENV_MEDIA_UPLOAD_THREAD_COUNT) {
            builder =
                builder.media_upload_concurrency(parse_count(ENV_MEDIA_UPLOAD_THREAD_COUNT, &raw)?);
        }
        Ok(builder)
    }

    /// Override the Langfuse API base URL (default: `https://cloud.langfuse.com`).
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the HTTP request timeout (default: 30 s).
    ///
    /// Also applied per request when a custom
    /// [`http_client`](Self::http_client) is used.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set a custom retry policy (default: 2 retries with exponential backoff).
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// Add an extra default header sent with every request.
    ///
    /// An invalid header name or value makes [`build`](Self::build) return
    /// [`LangfuseError::Config`].
    pub fn header(mut self, name: &str, value: &str) -> Self {
        match (name.parse::<HeaderName>(), HeaderValue::from_str(value)) {
            (Ok(header_name), Ok(header_value)) => {
                self.extra_headers.insert(header_name, header_value);
            }
            (Err(e), _) => {
                self.header_error
                    .get_or_insert_with(|| format!("Invalid header name '{name}': {e}"));
            }
            (_, Err(e)) => {
                self.header_error
                    .get_or_insert_with(|| format!("Invalid value for header '{name}': {e}"));
            }
        }
        self
    }

    /// Default time-to-live of prompts cached by
    /// [`PromptsClient::get_cached`](crate::api::prompts::PromptsClient::get_cached)
    /// (default: 60 s). `Duration::ZERO` disables caching unless a call sets its own TTL.
    pub fn prompt_cache_ttl(mut self, ttl: Duration) -> Self {
        self.prompt_cache_ttl = Some(ttl);
        self
    }

    /// Batching of [`LangfuseClient::score`](crate::LangfuseClient::score).
    /// `max_batch_bytes` above 1,000,000 is capped to it.
    pub fn score_queue(mut self, options: ScoreQueueOptions) -> Self {
        self.score_queue = Some(options);
        self
    }

    /// Number of media uploads the background queue runs at the same time
    /// (default: 1). The queue uploads the media found in exported spans
    /// (feature `otel`). `0` makes [`build`](Self::build) fail.
    pub fn media_upload_concurrency(mut self, uploads: usize) -> Self {
        self.media_upload_concurrency = Some(uploads);
        self
    }

    /// Send requests through this [`reqwest::Client`] instead of one built by
    /// the SDK (useful for proxies, custom TLS or sharing a connection pool).
    ///
    /// Authentication, SDK headers and retries still apply. The client's own
    /// timeout is kept unless [`timeout`](Self::timeout) is also set.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Validate and build the final [`ClientConfig`].
    pub fn build(self) -> Result<ClientConfig, LangfuseError> {
        if let Some(msg) = self.header_error {
            return Err(LangfuseError::Config(msg));
        }

        let mut score_queue = self.score_queue.unwrap_or_default();
        if score_queue.flush_at == 0
            || score_queue.max_queue_size == 0
            || score_queue.max_batch_bytes == 0
            || score_queue.flush_interval.is_zero()
        {
            return Err(LangfuseError::Config(
                "Invalid score queue options: every value must be greater than 0".into(),
            ));
        }
        if score_queue.max_batch_bytes > SCORES_BODY_LIMIT_BYTES {
            tracing::warn!(
                max_batch_bytes = score_queue.max_batch_bytes,
                "score batches are capped at the 1,000,000-byte limit of the endpoint"
            );
            score_queue.max_batch_bytes = SCORES_BODY_LIMIT_BYTES;
        }

        let media_upload_concurrency = self.media_upload_concurrency.unwrap_or(1);
        if media_upload_concurrency == 0 {
            return Err(LangfuseError::Config(
                "Invalid media upload concurrency: it must be greater than 0".into(),
            ));
        }

        let raw_url = self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        let mut base_url = Url::parse(raw_url)
            .map_err(|e| LangfuseError::Config(format!("Invalid base URL '{raw_url}': {e}")))?;
        // A trailing slash makes request paths join under a base path prefix
        // (`https://host/langfuse` → `https://host/langfuse/api/public/...`).
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        let credentials = format!("{}:{}", self.public_key, self.secret_key);
        let encoded = STANDARD.encode(credentials.as_bytes());
        let mut auth_value = HeaderValue::from_str(&format!("Basic {encoded}"))
            .map_err(|e| LangfuseError::Config(format!("Invalid auth credentials: {e}")))?;
        // Printed as `Sensitive` by `Debug`, so `{:?}` of the config never
        // shows the (barely encoded) secret key.
        auth_value.set_sensitive(true);

        let mut default_headers = self.extra_headers;
        default_headers.insert("x-langfuse-sdk-name", HeaderValue::from_static(SDK_NAME));
        if let Ok(v) = HeaderValue::from_str(SDK_VERSION) {
            default_headers.insert("x-langfuse-sdk-version", v);
        }
        if let Ok(v) = HeaderValue::from_str(&self.public_key) {
            default_headers.insert("x-langfuse-public-key", v);
        }

        Ok(ClientConfig {
            base_url,
            auth_header: auth_value,
            public_key: self.public_key,
            default_headers,
            timeout: self.timeout,
            retry_policy: self.retry_policy.unwrap_or_default(),
            http_client: self.http_client,
            prompt_cache_ttl: self.prompt_cache_ttl.unwrap_or(DEFAULT_PROMPT_CACHE_TTL),
            score_queue,
            media_upload_concurrency,
        })
    }
}

/// Parse a positive integer from environment variable `name`.
fn parse_count(name: &str, raw: &str) -> Result<usize, LangfuseError> {
    raw.trim()
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            LangfuseError::Config(format!(
                "Invalid {name} '{raw}': expected a positive integer"
            ))
        })
}

/// Parse a duration in seconds (integer or decimal) from environment variable `name`.
fn parse_secs(name: &str, raw: &str, allow_zero: bool) -> Result<Duration, LangfuseError> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|secs| *secs > 0.0 || (allow_zero && *secs == 0.0))
        .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
        .ok_or_else(|| {
            let bound = if allow_zero {
                "0 or more"
            } else {
                "greater than 0"
            };
            LangfuseError::Config(format!(
                "Invalid {name} '{raw}': expected a number of seconds {bound}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use super::{ConfigBuilder, STANDARD};
    use crate::error::LangfuseError;
    use base64::Engine;

    fn from_vars(vars: &[(&str, &str)]) -> Result<ConfigBuilder, LangfuseError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ConfigBuilder::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn media_upload_concurrency_from_env_and_builder() {
        let config = from_vars(&KEYS).unwrap().build().unwrap();
        assert_eq!(config.media_upload_concurrency(), 1);
        let config = from_vars(&[
            KEYS[0],
            KEYS[1],
            ("LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT", "4"),
        ])
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(config.media_upload_concurrency(), 4);
        for raw in ["0", "-1", "two"] {
            let msg = config_error(from_vars(&[
                KEYS[0],
                KEYS[1],
                ("LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT", raw),
            ]));
            assert!(
                msg.contains("LANGFUSE_MEDIA_UPLOAD_THREAD_COUNT"),
                "{raw}: {msg}"
            );
        }
        let err = super::ClientConfig::builder("pk", "sk")
            .media_upload_concurrency(0)
            .build()
            .unwrap_err();
        assert!(matches!(err, LangfuseError::Config(msg) if msg.contains("media upload")));
    }

    fn config_error(result: Result<ConfigBuilder, LangfuseError>) -> String {
        match result {
            Err(LangfuseError::Config(msg)) => msg,
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected Config error, got Ok"),
        }
    }

    const KEYS: [(&str, &str); 2] = [
        ("LANGFUSE_PUBLIC_KEY", "pk-lf-1"),
        ("LANGFUSE_SECRET_KEY", "sk-lf-1"),
    ];

    #[test]
    fn from_env_reads_keys_and_defaults() {
        let config = from_vars(&KEYS).unwrap().build().unwrap();
        assert_eq!(config.base_url().as_str(), "https://cloud.langfuse.com/");
        assert_eq!(config.timeout(), Duration::from_secs(30));
        assert_eq!(config.public_key, "pk-lf-1");
    }

    #[test]
    fn from_env_reads_base_url_and_timeout() {
        let config = from_vars(&[
            KEYS[0],
            KEYS[1],
            ("LANGFUSE_BASE_URL", "https://us.cloud.langfuse.com"),
            ("LANGFUSE_HOST", "https://ignored.example.com"),
            ("LANGFUSE_TIMEOUT", "2.5"),
        ])
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(config.base_url().as_str(), "https://us.cloud.langfuse.com/");
        assert_eq!(config.timeout(), Duration::from_millis(2500));
    }

    #[test]
    fn from_env_falls_back_to_deprecated_host() {
        let config = from_vars(&[
            KEYS[0],
            KEYS[1],
            ("LANGFUSE_BASE_URL", ""),
            ("LANGFUSE_HOST", "https://self-hosted.example.com"),
        ])
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(
            config.base_url().as_str(),
            "https://self-hosted.example.com/"
        );
    }

    #[test]
    fn from_env_missing_or_empty_keys_are_errors() {
        let msg = config_error(from_vars(&[KEYS[1]]));
        assert_eq!(msg, "LANGFUSE_PUBLIC_KEY is not set");

        let msg = config_error(from_vars(&[KEYS[0], ("LANGFUSE_SECRET_KEY", "  ")]));
        assert_eq!(msg, "LANGFUSE_SECRET_KEY is not set");
    }

    #[test]
    fn from_env_rejects_invalid_timeouts() {
        for raw in ["abc", "0", "-3", "NaN", "inf"] {
            let msg = config_error(from_vars(&[KEYS[0], KEYS[1], ("LANGFUSE_TIMEOUT", raw)]));
            assert!(msg.contains("LANGFUSE_TIMEOUT"), "{raw}: {msg}");
        }
    }

    #[test]
    fn from_env_reads_prompt_cache_ttl() {
        let config = from_vars(&KEYS).unwrap().build().unwrap();
        assert_eq!(config.prompt_cache_ttl(), Duration::from_secs(60));

        for (raw, expected) in [("300", 300_000), ("0", 0), ("1.5", 1_500)] {
            let config = from_vars(&[
                KEYS[0],
                KEYS[1],
                ("LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS", raw),
            ])
            .unwrap()
            .build()
            .unwrap();
            assert_eq!(config.prompt_cache_ttl(), Duration::from_millis(expected));
        }

        for raw in ["-1", "soon"] {
            let msg = config_error(from_vars(&[
                KEYS[0],
                KEYS[1],
                ("LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS", raw),
            ]));
            assert!(
                msg.contains("LANGFUSE_PROMPT_CACHE_DEFAULT_TTL_SECONDS"),
                "{msg}"
            );
        }
    }

    #[test]
    fn from_env_reads_score_queue_options() {
        let config = from_vars(&KEYS).unwrap().build().unwrap();
        assert_eq!(config.score_queue(), &super::ScoreQueueOptions::default());

        let config = from_vars(&[
            KEYS[0],
            KEYS[1],
            ("LANGFUSE_FLUSH_AT", "50"),
            ("LANGFUSE_FLUSH_INTERVAL", "0.25"),
            ("LANGFUSE_MAX_BATCH_SIZE_BYTES", "5000000"),
        ])
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(config.score_queue().flush_at, 50);
        assert_eq!(
            config.score_queue().flush_interval,
            Duration::from_millis(250)
        );
        assert_eq!(config.score_queue().max_batch_bytes, 1_000_000, "capped");

        for (name, raw) in [
            ("LANGFUSE_FLUSH_AT", "0"),
            ("LANGFUSE_MAX_BATCH_SIZE_BYTES", "x"),
        ] {
            let msg = config_error(from_vars(&[KEYS[0], KEYS[1], (name, raw)]));
            assert!(msg.contains(name), "{msg}");
        }
    }

    #[test]
    fn from_env_builder_can_be_overridden() {
        let config = from_vars(&KEYS)
            .unwrap()
            .base_url("https://override.example.com")
            .timeout(Duration::from_secs(7))
            .build()
            .unwrap();
        assert_eq!(config.base_url().as_str(), "https://override.example.com/");
        assert_eq!(config.timeout(), Duration::from_secs(7));
    }

    #[test]
    fn debug_output_hides_the_credentials() {
        let config = super::ClientConfig::builder("pk-lf-debug", "sk-lf-secret-debug")
            .build()
            .unwrap();
        let encoded = STANDARD.encode("pk-lf-debug:sk-lf-secret-debug");
        let printed = format!("{config:?}");
        assert!(!printed.contains(&encoded), "{printed}");
        assert!(!printed.contains("sk-lf-secret-debug"), "{printed}");
        assert!(printed.contains("Sensitive"), "{printed}");
        // Still sent as is.
        assert_eq!(
            config.auth_header.to_str().unwrap(),
            format!("Basic {encoded}")
        );
    }
}
