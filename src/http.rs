//! Low-level HTTP transport shared by all API clients.

use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, Method, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
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

/// Per-request overrides of the client defaults.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestOptions<'a> {
    /// Extra headers for this request.
    pub headers: Option<&'a HeaderMap>,
    /// Retries for this request instead of `RetryPolicy::max_retries`.
    pub max_retries: Option<u32>,
    /// Timeout for this request instead of the client timeout.
    pub timeout: Option<Duration>,
    /// Whether the request can be repeated without duplicating its effect,
    /// instead of deciding from the HTTP method.
    pub idempotent: Option<bool>,
}

/// Thin wrapper around [`reqwest::Client`] that adds authentication,
/// default headers, and a retry loop to every request.
pub struct HttpClient {
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
            client,
            base_url: config.base_url,
            auth_header: config.auth_header,
            default_headers: config.default_headers,
            retry_policy: config.retry_policy,
            request_timeout,
        })
    }

    /// The configured API base URL.
    pub(crate) fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// The underlying `reqwest` client, for requests outside the Langfuse API
    /// (presigned media URLs) that must not carry its credentials.
    pub(crate) fn raw_client(&self) -> &Client {
        &self.client
    }

    /// The retry policy of the client.
    pub(crate) fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    /// POST with per-request options; deserializes the JSON response.
    pub(crate) async fn post_with_options<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        options: RequestOptions<'_>,
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
            RequestOptions::default(),
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
            RequestOptions::default(),
        )
        .await
    }

    /// GET with query parameters and per-request retry/timeout overrides.
    pub(crate) async fn get_with_query_options<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
        options: RequestOptions<'_>,
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
            RequestOptions::default(),
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
            RequestOptions::default(),
        )
        .await
    }

    /// POST with per-request options (headers, idempotency); the response body is ignored.
    pub(crate) async fn post_no_response_with_options<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        options: RequestOptions<'_>,
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
            RequestOptions {
                headers: Some(headers),
                ..RequestOptions::default()
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
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
            RequestOptions::default(),
        )
        .await
    }

    fn build_url(&self, path: &str) -> Result<Url, LangfuseError> {
        // Join as a relative path so a base path prefix (`https://host/langfuse/`)
        // is kept; the base URL always ends with `/` (see `ConfigBuilder::build`).
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(LangfuseError::UrlParse)
    }

    fn apply_common_headers(&self, mut builder: RequestBuilder) -> RequestBuilder {
        builder = builder.header(AUTHORIZATION, self.auth_header.clone());
        for (name, value) in &self.default_headers {
            builder = builder.header(name, value);
        }
        builder
    }

    /// Send a request and deserialize the JSON body of a 2xx response.
    async fn request<T, B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        options: RequestOptions<'_>,
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
        options: RequestOptions<'_>,
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
        options: RequestOptions<'_>,
    ) -> Result<Vec<u8>, LangfuseError>
    where
        B: Serialize,
        Q: Serialize,
    {
        let url = self.build_url(path)?;
        let timeout = options.timeout.or(self.request_timeout);
        let max_retries = options.max_retries.unwrap_or(self.retry_policy.max_retries);
        let idempotent = options.idempotent.unwrap_or_else(|| is_idempotent(&method));
        let mut attempt = 0;

        loop {
            debug!(method = %method, url = %url, attempt, "sending request");

            let mut builder = self.client.request(method.clone(), url.clone());
            builder = self.apply_common_headers(builder);
            if let Some(timeout) = timeout {
                builder = builder.timeout(timeout);
            }

            if let Some(extra) = options.headers {
                for (name, value) in extra {
                    builder = builder.header(name, value);
                }
            }
            if let Some(q) = query {
                builder = builder.query(q);
            }
            if let Some(b) = body {
                builder = builder.json(b);
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
                    let delay = self.retry_policy.delay(attempt, None);
                    debug!(attempt, delay_ms = delay.as_millis(), error = %err, "retrying");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Err(err) => return Err(LangfuseError::Transport(err)),
            };

            if status.is_success() {
                return Ok(bytes.to_vec());
            }

            if attempt >= max_retries || !RetryPolicy::is_retryable_for(status, idempotent) {
                return Err(map_response_error(status, &bytes, &response_headers));
            }

            let retry_after = parse_retry_after_header(&response_headers);
            let delay = self.retry_policy.delay(attempt, retry_after);
            debug!(attempt, delay_ms = delay.as_millis(), status = %status, "retrying");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }
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
