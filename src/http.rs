use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Method, RequestBuilder};
use serde::{Serialize, de::DeserializeOwned};
use tracing::debug;
use url::Url;

use crate::config::ClientConfig;
use crate::error::{LangfuseError, map_response_error};
use crate::retry::RetryPolicy;

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
        self.request(Method::GET, path, None::<&()>, None::<&()>)
            .await
    }

    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T, LangfuseError> {
        self.request(Method::GET, path, None::<&()>, Some(query))
            .await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::POST, path, Some(body), None::<&()>)
            .await
    }

    pub async fn post_no_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::POST, path, Some(body)).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::PUT, path, Some(body), None::<&()>)
            .await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LangfuseError> {
        self.request(Method::PATCH, path, Some(body), None::<&()>)
            .await
    }

    pub async fn delete_no_content(&self, path: &str) -> Result<(), LangfuseError> {
        self.request_no_content(Method::DELETE, path, None::<&()>)
            .await
    }

    pub async fn delete_with_body<B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), LangfuseError> {
        self.request_no_content(Method::DELETE, path, Some(body))
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

    async fn request<T, B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
    ) -> Result<T, LangfuseError>
    where
        T: DeserializeOwned,
        B: Serialize,
        Q: Serialize,
    {
        let url = self.build_url(path)?;
        let mut attempt = 0;

        loop {
            debug!(method = %method, url = %url, attempt, "sending request");

            let mut builder = self.client.request(method.clone(), url.clone());
            builder = self.apply_common_headers(builder);

            if let Some(q) = query {
                builder = builder.query(q);
            }
            if let Some(b) = body {
                builder = builder.json(b);
            }

            let response = builder.send().await.map_err(LangfuseError::Transport)?;
            let status = response.status();

            if status.is_success() {
                let bytes = response.bytes().await.map_err(LangfuseError::Transport)?;
                return serde_json::from_slice(&bytes).map_err(LangfuseError::Serialization);
            }

            let headers = response.headers().clone();
            let bytes = response.bytes().await.map_err(LangfuseError::Transport)?;

            if !self.retry_policy.should_retry(status, attempt) {
                return Err(map_response_error(status, &bytes, &headers));
            }

            let retry_after = parse_retry_after_header(&headers);
            let delay = self.retry_policy.delay(attempt, retry_after);
            debug!(attempt, delay_ms = delay.as_millis(), status = %status, "retrying");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }

    async fn request_no_content<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<(), LangfuseError> {
        let url = self.build_url(path)?;
        let mut attempt = 0;

        loop {
            debug!(method = %method, url = %url, attempt, "sending request");

            let mut builder = self.client.request(method.clone(), url.clone());
            builder = self.apply_common_headers(builder);

            if let Some(b) = body {
                builder = builder.json(b);
            }

            let response = builder.send().await.map_err(LangfuseError::Transport)?;
            let status = response.status();

            if status.is_success() {
                return Ok(());
            }

            let headers = response.headers().clone();
            let bytes = response.bytes().await.map_err(LangfuseError::Transport)?;

            if !self.retry_policy.should_retry(status, attempt) {
                return Err(map_response_error(status, &bytes, &headers));
            }

            let retry_after = parse_retry_after_header(&headers);
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
