use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderMap, HeaderValue};
use url::Url;

use crate::error::LangfuseError;
use crate::retry::RetryPolicy;

const DEFAULT_BASE_URL: &str = "https://cloud.langfuse.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const SDK_NAME: &str = "rust";
const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub struct ClientConfig {
    pub(crate) base_url: Url,
    pub(crate) auth_header: HeaderValue,
    #[allow(dead_code)]
    pub(crate) public_key: String,
    pub(crate) default_headers: HeaderMap,
    pub(crate) timeout: Duration,
    pub(crate) retry_policy: RetryPolicy,
}

impl ClientConfig {
    pub fn builder(public_key: impl Into<String>, secret_key: impl Into<String>) -> ConfigBuilder {
        ConfigBuilder {
            public_key: public_key.into(),
            secret_key: secret_key.into(),
            base_url: None,
            timeout: None,
            retry_policy: None,
            extra_headers: HeaderMap::new(),
        }
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

pub struct ConfigBuilder {
    public_key: String,
    secret_key: String,
    base_url: Option<String>,
    timeout: Option<Duration>,
    retry_policy: Option<RetryPolicy>,
    extra_headers: HeaderMap,
}

impl ConfigBuilder {
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        if let (Ok(name), Ok(value)) = (
            name.parse::<reqwest::header::HeaderName>(),
            HeaderValue::from_str(value),
        ) {
            self.extra_headers.insert(name, value);
        }
        self
    }

    pub fn build(self) -> Result<ClientConfig, LangfuseError> {
        let raw_url = self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        let base_url = Url::parse(raw_url)
            .map_err(|e| LangfuseError::Config(format!("Invalid base URL '{raw_url}': {e}")))?;

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
            timeout: self.timeout.unwrap_or(DEFAULT_TIMEOUT),
            retry_policy: self.retry_policy.unwrap_or_default(),
        })
    }
}
