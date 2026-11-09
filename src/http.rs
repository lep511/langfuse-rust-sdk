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

pub struct HttpClient {
    client: Client,
    base_url: Url,
    auth_header: HeaderValue,
    default_headers: HeaderMap,
    retry_policy: RetryPolicy,
}

impl HttpClient {
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(LangfuseError::Transport)?;

        Ok(Self {
            client,
            base_url: config.base_url,
            auth_header: config.auth_header,
            default_headers: config.default_headers,
            retry_policy: config.retry_policy,
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, LangfuseError> {
        self.request(Method::GET, path, None::<&()>, None::<&()>, None)
            .await
    }

    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T, LangfuseError> {
        self.request(Method::GET, path, None::<&()>, Some(query), None)
            .await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::POST, path, Some(body), None::<&()>, None)
            .await
    }

    pub async fn post_no_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::POST, path, Some(body), None::<&()>, None)
            .await
    }

    /// POST with extra per-request headers; the response body is ignored.
    pub async fn post_no_response_with_headers<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        headers: &HeaderMap,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::POST, path, Some(body), None::<&()>, Some(headers))
            .await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::PUT, path, Some(body), None::<&()>, None)
            .await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::PATCH, path, Some(body), None::<&()>, None)
            .await
    }

    pub async fn patch_no_content<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::PATCH, path, Some(body), None::<&()>, None)
            .await
    }

    pub async fn delete_no_content(&self, path: &str) -> Result<(), LangfuseError> {
        self.request_no_content(Method::DELETE, path, None::<&()>, None::<&()>, None)
            .await
    }

    pub async fn delete_no_content_with_query<Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::DELETE, path, None::<&()>, Some(query), None)
            .await
    }

    /// DELETE with a JSON body; the response body is ignored.
    pub async fn delete_with_body<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::DELETE, path, Some(body), None::<&()>, None)
            .await
    }

    /// DELETE without a body that returns a JSON response.
    pub async fn delete_with_response<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, LangfuseError> {
        self.request(Method::DELETE, path, None::<&()>, None::<&()>, None)
            .await
    }

    /// DELETE with a JSON body that returns a JSON response.
    pub async fn delete_with_body_response<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::DELETE, path, Some(body), None::<&()>, None)
            .await
    }

    fn build_url(&self, path: &str) -> Result<Url, LangfuseError> {
        self.base_url.join(path).map_err(LangfuseError::UrlParse)
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
        headers: Option<&HeaderMap>,
    ) -> Result<T, LangfuseError>
    where
        T: DeserializeOwned,
        B: Serialize,
        Q: Serialize,
    {
        let bytes = self.execute(method, path, body, query, headers).await?;
        serde_json::from_slice(&bytes).map_err(LangfuseError::Serialization)
    }

    /// Send a request and discard the body of a 2xx response (204 and similar).
    async fn request_no_content<B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        headers: Option<&HeaderMap>,
    ) -> Result<(), LangfuseError>
    where
        B: Serialize,
        Q: Serialize,
    {
        self.execute(method, path, body, query, headers).await?;
        Ok(())
    }

    /// Retry loop shared by every request. Returns the raw body of a 2xx response.
    async fn execute<B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
        headers: Option<&HeaderMap>,
    ) -> Result<Vec<u8>, LangfuseError>
    where
        B: Serialize,
        Q: Serialize,
    {
        let url = self.build_url(path)?;
        let mut attempt = 0;

        loop {
            debug!(method = %method, url = %url, attempt, "sending request");

            let mut builder = self.client.request(method.clone(), url.clone());
            builder = self.apply_common_headers(builder);

            if let Some(extra) = headers {
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

            let response = builder.send().await.map_err(LangfuseError::Transport)?;
            let status = response.status();
            let response_headers = response.headers().clone();
            let bytes = response.bytes().await.map_err(LangfuseError::Transport)?;

            if status.is_success() {
                return Ok(bytes.to_vec());
            }

            if !self.retry_policy.should_retry(status, attempt) {
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
