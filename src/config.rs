//! SDK configuration and builder.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::error::LangfuseError;
use crate::retry::RetryPolicy;

const DEFAULT_BASE_URL: &str = "https://cloud.langfuse.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
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

/// Resolved configuration consumed by [`LangfuseClient::new`](crate::LangfuseClient::new).
///
/// Construct one via [`ClientConfig::builder`] or [`ClientConfig::from_env`].
#[derive(Debug)]
pub struct ClientConfig {
    pub(crate) base_url: Url,
    pub(crate) auth_header: HeaderValue,
    #[allow(dead_code)]
    pub(crate) public_key: String,
    pub(crate) default_headers: HeaderMap,
    /// `None` when [`ConfigBuilder::timeout`] was never called.
    pub(crate) timeout: Option<Duration>,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) http_client: Option<reqwest::Client>,
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
    ///
    /// Empty values count as unset. Missing keys and an invalid
    /// `LANGFUSE_TIMEOUT` return [`LangfuseError::Config`]. The variables are
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
            builder = builder.timeout(parse_timeout_secs(&raw)?);
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
        let auth_value = HeaderValue::from_str(&format!("Basic {encoded}"))
            .map_err(|e| LangfuseError::Config(format!("Invalid auth credentials: {e}")))?;

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
        })
    }
}

/// Parse `LANGFUSE_TIMEOUT` (seconds, integer or decimal, greater than zero).
fn parse_timeout_secs(raw: &str) -> Result<Duration, LangfuseError> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|secs| *secs > 0.0)
        .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
        .ok_or_else(|| {
            LangfuseError::Config(format!(
                "Invalid {ENV_TIMEOUT} '{raw}': expected a number of seconds greater than 0"
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use super::ConfigBuilder;
    use crate::error::LangfuseError;

    fn from_vars(vars: &[(&str, &str)]) -> Result<ConfigBuilder, LangfuseError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ConfigBuilder::from_lookup(|name| vars.get(name).cloned())
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
}
