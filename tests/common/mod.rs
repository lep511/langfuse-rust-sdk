//! Shared helpers for wiremock-based HTTP integration tests.
#![allow(dead_code)]

use std::time::Duration;

use langfuse_sdk::{ClientConfig, LangfuseClient, RetryPolicy};
use serde_json::Value;
use wiremock::{MockServer, Request, ResponseTemplate};

pub const PUBLIC_KEY: &str = "pk-test";
pub const SECRET_KEY: &str = "sk-test";

/// Client pointed at a fresh mock server, with retries disabled.
pub async fn test_client() -> (LangfuseClient, MockServer) {
    test_client_with_retries(0).await
}

/// Client pointed at a fresh mock server, with `n` fast retries (1 ms backoff, no jitter).
pub async fn test_client_with_retries(n: u32) -> (LangfuseClient, MockServer) {
    let server = MockServer::start().await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .retry_policy(RetryPolicy {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            max_retries: n,
            jitter_fraction: 0.0,
        })
        .build()
        .expect("valid test config");
    let client = LangfuseClient::new(config).expect("client builds");
    (client, server)
}

/// JSON response with the given status code.
pub fn json(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

/// The single request received by the server (panics if there is not exactly one).
pub async fn single_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 1, "expected exactly one request");
    requests.remove(0)
}

/// Request body parsed as JSON.
pub fn body_json(request: &Request) -> Value {
    serde_json::from_slice(&request.body).expect("request body is JSON")
}

/// Query pairs of a request, in order, percent-decoded.
pub fn query_pairs(request: &Request) -> Vec<(String, String)> {
    request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Value of a single query parameter (panics if absent).
pub fn query_value(request: &Request, key: &str) -> String {
    query_pairs(request)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("query param `{key}` missing"))
}

/// Canonical timestamp used in fixtures.
pub const TS: &str = "2024-06-15T10:30:00Z";
