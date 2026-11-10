//! Low-level HTTP transport shared by all API clients.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use tracing::debug;
use url::Url;

use crate::config::ClientConfig;
use crate::error::{LangfuseError, map_response_error};
use crate::retry::RetryPolicy;

/// Characters left unencoded in a path segment: RFC 3986 "unreserved" (`A-Z a-z 0-9 - . _ ~`).
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Percent-encode a value for use as a single URL path segment.
///
/// Slashes are encoded too, so a prompt named `folder/sub/name` becomes
/// `folder%2Fsub%2Fname`, as the Langfuse API expects.
pub(crate) fn encode_path_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT).to_string()
}

/// Called with every HTTP response of the Langfuse API; see
/// [`RequestOptions::with_response_hook`].
pub type ResponseHook = Arc<dyn Fn(&ResponseInfo<'_>) + Send + Sync>;

/// One HTTP response received from the Langfuse API, passed to a
/// [`ResponseHook`] before the body is decoded.
#[derive(Debug)]
#[non_exhaustive]
pub struct ResponseInfo<'a> {
    /// Request method.
    pub method: &'a Method,
    /// Request URL, with its query.
    pub url: &'a Url,
    /// Response status.
    pub status: StatusCode,
    /// Response headers (e.g. rate-limit or request-ID headers).
    pub headers: &'a HeaderMap,
    /// `0` for the first attempt, then one more per retry.
    pub attempt: u32,
}

/// Overrides applied to every request made through
/// [`LangfuseClient::with_options`](crate::LangfuseClient::with_options):
/// timeout, retries, extra headers, query and body parameters, and a hook
/// that sees the response headers. The equivalent of `request_options` in
/// the Python SDK.
///
/// Build it from [`RequestOptions::default`] with the `with_*` methods:
///
/// ```
/// use langfuse_rust_sdk::RequestOptions;
/// use std::time::Duration;
///
/// let options = RequestOptions::default()
///     .with_timeout(Duration::from_secs(5))
///     .with_max_retries(0)
///     .with_header("x-request-source", "batch-job");
/// assert_eq!(options.max_retries, Some(0));
/// ```
///
/// Retries keep the rules of the [`RetryPolicy`]: only the number changes.
/// Methods with their own per-call settings
/// ([`get_cached`](crate::api::prompts::PromptsClient::get_cached)'s
/// `max_retries` and `fetch_timeout`, the page requests of
/// [`run_batched_evaluation`](crate::LangfuseClient::run_batched_evaluation))
/// keep them.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct RequestOptions {
    /// Timeout of each attempt, instead of the client timeout.
    pub timeout: Option<Duration>,
    /// Retries after the first attempt, instead of
    /// [`RetryPolicy::max_retries`].
    pub max_retries: Option<u32>,
    /// Headers added to every request; they replace headers of the same name
    /// (including the client's default headers).
    pub headers: HeaderMap,
    /// Query parameters added to every request; they replace parameters of
    /// the same name.
    pub query: Vec<(String, String)>,
    /// Fields merged into JSON object bodies (replacing fields of the same
    /// name), or sent as the body of requests without one. Other bodies (JSON
    /// arrays) are sent unchanged.
    pub body: Map<String, Value>,
    /// Called with every response, retried ones included.
    pub response_hook: Option<ResponseHook>,
    /// First invalid header given to [`with_header`](Self::with_header).
    header_error: Option<String>,
}

impl fmt::Debug for RequestOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestOptions")
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("headers", &self.headers)
            .field("query", &self.query)
            .field("body", &self.body)
            .field(
                "response_hook",
                &self.response_hook.as_ref().map(|_| "<fn>"),
            )
            .finish_non_exhaustive()
    }
}

impl RequestOptions {
    /// Set [`timeout`](Self::timeout).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set [`max_retries`](Self::max_retries).
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Add one header to [`headers`](Self::headers). An invalid name or value
    /// makes every request with these options fail with
    /// [`LangfuseError::Config`], like [`ConfigBuilder::header`](crate::ConfigBuilder::header)
    /// makes `build` fail.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        match (name.parse::<HeaderName>(), HeaderValue::from_str(value)) {
            (Ok(name), Ok(value)) => {
                self.headers.insert(name, value);
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

    /// Add the parameter `name=value` to [`query`](Self::query), replacing
    /// an earlier one of the same name.
    pub fn with_query_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into();
        self.query.retain(|(k, _)| *k != name);
        self.query.push((name, value.into()));
        self
    }

    /// Set the field `name` of [`body`](Self::body).
    pub fn with_body_param(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.body.insert(name.into(), value.into());
        self
    }

    /// Set [`response_hook`](Self::response_hook), e.g. to read rate-limit
    /// headers.
    ///
    /// ```
    /// use langfuse_rust_sdk::RequestOptions;
    ///
    /// let options = RequestOptions::default().with_response_hook(|response| {
    ///     if let Some(remaining) = response.headers.get("x-ratelimit-remaining") {
    ///         println!("{} {}: {remaining:?} left", response.method, response.url);
    ///     }
    /// });
    /// # let _ = options;
    /// ```
    pub fn with_response_hook<F>(mut self, hook: F) -> Self
    where
        F: Fn(&ResponseInfo<'_>) + Send + Sync + 'static,
    {
        self.response_hook = Some(Arc::new(hook));
        self
    }

    /// `self` overridden by `other`: values set in `other` win, headers,
    /// query and body parameters are merged.
    fn merged(&self, other: &RequestOptions) -> RequestOptions {
        let mut out = self.clone();
        out.timeout = other.timeout.or(out.timeout);
        out.max_retries = other.max_retries.or(out.max_retries);
        for (name, value) in &other.headers {
            out.headers.insert(name, value.clone());
        }
        for (name, value) in &other.query {
            out = out.with_query_param(name.clone(), value.clone());
        }
        out.body
            .extend(other.body.iter().map(|(k, v)| (k.clone(), v.clone())));
        if other.response_hook.is_some() {
            out.response_hook = other.response_hook.clone();
        }
        if out.header_error.is_none() {
            out.header_error = other.header_error.clone();
        }
        out
    }
}

/// Per-call overrides set by the SDK itself; they win over [`RequestOptions`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CallOptions<'a> {
    /// Extra headers for this request.
    pub headers: Option<&'a HeaderMap>,
    /// Retries for this request instead of `RetryPolicy::max_retries`.
    pub max_retries: Option<u32>,
    /// Timeout for this request instead of the client timeout.
    pub timeout: Option<Duration>,
    /// Whether the request can be repeated without duplicating its effect,
    /// instead of deciding from the HTTP method.
    pub idempotent: Option<bool>,
    /// Send the JSON body gzip-compressed with `Content-Encoding: gzip`
    /// (compressed once, before the first attempt).
    #[cfg(feature = "otel")]
    pub gzip: bool,
}

/// Thin wrapper around [`reqwest::Client`] that adds authentication,
/// default headers, and a retry loop to every request.
pub struct HttpClient {
    shared: Arc<Shared>,
    /// Overrides of [`LangfuseClient::with_options`](crate::LangfuseClient::with_options).
    options: RequestOptions,
}

/// Configuration shared by an [`HttpClient`] and its views.
struct Shared {
    client: Client,
    base_url: Url,
    auth_header: HeaderValue,
    default_headers: HeaderMap,
    retry_policy: RetryPolicy,
    /// Per-request timeout, set only for a custom client with an explicit timeout
    /// (the SDK-built client carries its timeout itself).
    request_timeout: Option<Duration>,
}

impl HttpClient {
    /// Build a new HTTP client from resolved configuration.
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let timeout = config.timeout();
        let (client, request_timeout) = match config.http_client {
            Some(client) => (client, config.timeout),
            None => {
                let client = Client::builder()
                    .timeout(timeout)
                    .build()
                    .map_err(LangfuseError::Transport)?;
                (client, None)
            }
        };

        Ok(Self {
            shared: Arc::new(Shared {
                client,
                base_url: config.base_url,
                auth_header: config.auth_header,
                default_headers: config.default_headers,
                retry_policy: config.retry_policy,
                request_timeout,
            }),
            options: RequestOptions::default(),
        })
    }

    /// A client sharing this one's connection pool and configuration, with
    /// `options` applied on top of this client's own overrides.
    pub fn with_options(&self, options: &RequestOptions) -> HttpClient {
        HttpClient {
            shared: Arc::clone(&self.shared),
            options: self.options.merged(options),
        }
    }

    /// The overrides applied to every request of this client.
    pub fn options(&self) -> &RequestOptions {
        &self.options
    }

    /// The configured API base URL.
    pub(crate) fn base_url(&self) -> &Url {
        &self.shared.base_url
    }

    /// The underlying `reqwest` client, for requests outside the Langfuse API
    /// (presigned media URLs) that must not carry its credentials.
    pub(crate) fn raw_client(&self) -> &Client {
        &self.shared.client
    }

    /// The retry policy of the client.
    pub(crate) fn retry_policy(&self) -> &RetryPolicy {
        &self.shared.retry_policy
    }

    /// POST with per-request options; deserializes the JSON response.
    pub(crate) async fn post_with_options<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        options: CallOptions<'_>,
    ) -> Result<T, LangfuseError> {
        self.request(Method::POST, path, Some(body), None::<&()>, options)
            .await
    }

    /// Send a GET request and deserialize the JSON response.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, LangfuseError> {
        self.request(
            Method::GET,
            path,
            None::<&()>,
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a GET request with query parameters and deserialize the JSON response.
    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::GET,
            path,
            None::<&()>,
            Some(query),
            CallOptions::default(),
        )
        .await
    }

    /// GET with query parameters and per-request retry/timeout overrides.
    pub(crate) async fn get_with_query_options<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
        options: CallOptions<'_>,
    ) -> Result<T, LangfuseError> {
        self.request(Method::GET, path, None::<&()>, Some(query), options)
            .await
    }

    /// Send a POST request with a JSON body and deserialize the JSON response.
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::POST,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a POST request with a JSON body; the response body is ignored.
    pub async fn post_no_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::POST,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// POST with per-request options (headers, idempotency); the response body is ignored.
    pub(crate) async fn post_no_response_with_options<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        options: CallOptions<'_>,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::POST, path, Some(body), None::<&()>, options)
            .await
    }

    /// POST with extra per-request headers; the response body is ignored.
    pub async fn post_no_response_with_headers<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        headers: &HeaderMap,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::POST,
            path,
            Some(body),
            None::<&()>,
            CallOptions {
                headers: Some(headers),
                ..CallOptions::default()
            },
        )
        .await
    }

    /// Send a PUT request with a JSON body and deserialize the JSON response.
    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::PUT,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a PATCH request with a JSON body and deserialize the JSON response.
    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::PATCH,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a PATCH request with a JSON body; the response body is ignored.
    pub async fn patch_no_content<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::PATCH,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a DELETE request without a body; the response body is ignored.
    pub async fn delete_no_content(&self, path: &str) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::DELETE,
            path,
            None::<&()>,
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// Send a DELETE request with query parameters; the response body is ignored.
    pub async fn delete_no_content_with_query<Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::DELETE,
            path,
            None::<&()>,
            Some(query),
            CallOptions::default(),
        )
        .await
    }

    /// DELETE with a JSON body; the response body is ignored.
    pub async fn delete_with_body<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(
            Method::DELETE,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// DELETE without a body that returns a JSON response.
    pub async fn delete_with_response<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::DELETE,
            path,
            None::<&()>,
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// DELETE with a JSON body that returns a JSON response.
    pub async fn delete_with_body_response<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(
            Method::DELETE,
            path,
            Some(body),
            None::<&()>,
            CallOptions::default(),
        )
        .await
    }

    /// The request URL: `path` joined to the base URL, with the query and the
    /// extra query parameters of the overrides.
    fn build_url<Q: Serialize>(&self, path: &str, query: Option<&Q>) -> Result<Url, LangfuseError> {
        // Join as a relative path so a base path prefix (`https://host/langfuse/`)
        // is kept; the base URL always ends with `/` (see `ConfigBuilder::build`).
        let mut url = self
            .shared
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(LangfuseError::UrlParse)?;
        let mut pairs: Vec<(String, String)> = match query {
            Some(query) => {
                let encoded = serde_urlencoded::to_string(query)
                    .map_err(|e| LangfuseError::Config(format!("Invalid query parameters: {e}")))?;
                url::form_urlencoded::parse(encoded.as_bytes())
                    .into_owned()
                    .collect()
            }
            None => Vec::new(),
        };
        let extra = &self.options.query;
        pairs.retain(|(name, _)| !extra.iter().any(|(k, _)| k == name));
        pairs.extend(extra.iter().cloned());
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(&pairs);
        }
        Ok(url)
    }

    /// Default headers, then the per-call ones, then those of the overrides;
    /// later ones replace earlier ones of the same name.
    fn headers(&self, call: Option<&HeaderMap>) -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(self.shared.default_headers.len() + 1);
        headers.insert(AUTHORIZATION, self.shared.auth_header.clone());
        let layers = [
            Some(&self.shared.default_headers),
            call,
            Some(&self.options.headers),
        ];
        for layer in layers.into_iter().flatten() {
            for (name, value) in layer {
                headers.insert(name, value.clone());
            }
        }
        headers
    }

    /// The JSON body: `body` with the body parameters of the overrides merged in.
    fn json_body<B: Serialize>(&self, body: Option<&B>) -> Result<Option<Vec<u8>>, LangfuseError> {
        let extra = &self.options.body;
        let value = match body {
            None if extra.is_empty() => return Ok(None),
            None => Value::Object(extra.clone()),
            Some(body) if extra.is_empty() => return Ok(Some(serde_json::to_vec(body)?)),
            Some(body) => match serde_json::to_value(body)? {
                Value::Object(mut object) => {
                    object.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
                    Value::Object(object)
                }
                other => other,
            },
        };
        Ok(Some(serde_json::to_vec(&value)?))
    }

    /// Send a request and deserialize the JSON body of a 2xx response.
    async fn request<T, B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        options: CallOptions<'_>,
    ) -> Result<T, LangfuseError>
    where
        T: DeserializeOwned,
        B: Serialize,
        Q: Serialize,
    {
        let bytes = self.execute(method, path, body, query, options).await?;
        serde_json::from_slice(&bytes).map_err(LangfuseError::Serialization)
    }

    /// Send a request and discard the body of a 2xx response (204 and similar).
    async fn request_no_content<B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        options: CallOptions<'_>,
    ) -> Result<(), LangfuseError>
    where
        B: Serialize,
        Q: Serialize,
    {
        self.execute(method, path, body, query, options).await?;
        Ok(())
    }

    /// Retry loop shared by every request. Returns the raw body of a 2xx response.
    async fn execute<B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        options: CallOptions<'_>,
    ) -> Result<Vec<u8>, LangfuseError>
    where
        B: Serialize,
        Q: Serialize,
    {
        if let Some(message) = &self.options.header_error {
            return Err(LangfuseError::Config(message.clone()));
        }
        let shared = &*self.shared;
        let url = self.build_url(path, query)?;
        #[allow(unused_mut)]
        let mut headers = self.headers(options.headers);
        let body = self.json_body(body)?;
        #[cfg(feature = "otel")]
        let body = match body {
            Some(json) if options.gzip => {
                headers.insert(
                    reqwest::header::CONTENT_ENCODING,
                    HeaderValue::from_static("gzip"),
                );
                Some(gzip(&json)?)
            }
            other => other,
        };
        let timeout = options
            .timeout
            .or(self.options.timeout)
            .or(shared.request_timeout);
        let max_retries = options
            .max_retries
            .or(self.options.max_retries)
            .unwrap_or(shared.retry_policy.max_retries);
        let idempotent = options.idempotent.unwrap_or_else(|| is_idempotent(&method));
        let mut attempt = 0;

        loop {
            debug!(method = %method, url = %url, attempt, "sending request");

            let mut builder = shared
                .client
                .request(method.clone(), url.clone())
                .headers(headers.clone());
            if let Some(timeout) = timeout {
                builder = builder.timeout(timeout);
            }
            if let Some(body) = &body {
                if !headers.contains_key(CONTENT_TYPE) {
                    builder = builder.header(CONTENT_TYPE, "application/json");
                }
                builder = builder.body(body.clone());
            }

            let received = match builder.send().await {
                Ok(response) => {
                    let status = response.status();
                    let response_headers = response.headers().clone();
                    response
                        .bytes()
                        .await
                        .map(|bytes| (status, response_headers, bytes))
                }
                Err(err) => Err(err),
            };
            let (status, response_headers, bytes) = match received {
                Ok(received) => received,
                Err(err) if attempt < max_retries && retry_transport_error(&err, idempotent) => {
                    let delay = shared.retry_policy.delay(attempt, None);
                    debug!(attempt, delay_ms = delay.as_millis(), error = %err, "retrying");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Err(err) => return Err(LangfuseError::Transport(err)),
            };

            if let Some(hook) = &self.options.response_hook {
                hook(&ResponseInfo {
                    method: &method,
                    url: &url,
                    status,
                    headers: &response_headers,
                    attempt,
                });
            }

            if status.is_success() {
                return Ok(bytes.to_vec());
            }

            if attempt >= max_retries || !RetryPolicy::is_retryable_for(status, idempotent) {
                return Err(map_response_error(status, &bytes, &response_headers));
            }

            let retry_after = parse_retry_after_header(&response_headers);
            let delay = shared.retry_policy.delay(attempt, retry_after);
            debug!(attempt, delay_ms = delay.as_millis(), status = %status, "retrying");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }
}

/// `bytes` compressed as a gzip stream.
#[cfg(feature = "otel")]
fn gzip(bytes: &[u8]) -> Result<Vec<u8>, LangfuseError> {
    use std::io::Write as _;

    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(bytes.len() / 4),
        flate2::Compression::default(),
    );
    encoder
        .write_all(bytes)
        .and_then(|()| encoder.finish())
        .map_err(|e| LangfuseError::Config(format!("Failed to gzip the request body: {e}")))
}

/// Whether repeating a request with `method` cannot duplicate its effect.
/// PATCH is excluded: the HTTP spec does not make it idempotent.
fn is_idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::PUT | Method::DELETE | Method::OPTIONS
    )
}

/// Whether a transport error is worth retrying. A failed connection means the
/// request was never sent, so it is safe for every method; a timeout or a
/// dropped connection may hit after the server processed the request, so only
/// idempotent requests are repeated.
fn retry_transport_error(err: &reqwest::Error, idempotent: bool) -> bool {
    if err.is_builder() || err.is_redirect() {
        return false;
    }
    err.is_connect() || idempotent
}

fn parse_retry_after_header(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::encode_path_segment;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(
            encode_path_segment("folder/sub/name"),
            "folder%2Fsub%2Fname"
        );
        assert_eq!(encode_path_segment("a b"), "a%20b");
        assert_eq!(encode_path_segment("x?y#z%"), "x%3Fy%23z%25");
        assert_eq!(encode_path_segment("añ"), "a%C3%B1");
    }

    #[test]
    fn keeps_unreserved_characters() {
        assert_eq!(encode_path_segment("Abc-1.2_3~"), "Abc-1.2_3~");
    }
}
