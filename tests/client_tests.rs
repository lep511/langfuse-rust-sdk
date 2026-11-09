//! `LangfuseClient` helpers: `Clone`, `auth_check`, `trace_url` and custom HTTP clients.
mod common;

use std::time::Duration;

use common::*;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError, RetryPolicy};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn projects_body(ids: &[&str]) -> Value {
    let data: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "id": id, "name": "My Project",
                "organization": {"id": "org-1", "name": "Org"},
                "metadata": {}, "retentionDays": null
            })
        })
        .collect();
    json!({ "data": data })
}

async fn mount_projects(server: &MockServer, ids: &[&str], expected_calls: u64) {
    Mock::given(method("GET"))
        .and(path("/api/public/projects"))
        .respond_with(json(200, projects_body(ids)))
        .expect(expected_calls)
        .mount(server)
        .await;
}

fn client_with_base_url(base_url: String) -> LangfuseClient {
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(base_url)
        .build()
        .unwrap();
    LangfuseClient::new(config).unwrap()
}

// ═══════════════════════════════════════════════════════════════════════
//  auth_check / trace_url
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn auth_check_returns_the_project() {
    let (client, server) = test_client().await;
    mount_projects(&server, &["proj-1"], 1).await;

    let project = client.auth_check().await.unwrap();
    assert_eq!(project.id, "proj-1");
    assert_eq!(project.organization.name, "Org");
}

#[tokio::test]
async fn auth_check_without_projects_is_a_config_error() {
    let (client, server) = test_client().await;
    mount_projects(&server, &[], 1).await;

    match client.auth_check().await {
        Err(LangfuseError::Config(msg)) => assert!(msg.contains("no project found"), "{msg}"),
        other => panic!("expected Config error, got {other:?}"),
    }
}

#[tokio::test]
async fn auth_check_propagates_unauthorized() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/projects"))
        .respond_with(json(401, json!({"message": "Invalid credentials"})))
        .mount(&server)
        .await;

    assert!(matches!(
        client.auth_check().await,
        Err(LangfuseError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn trace_url_fetches_the_project_once_and_is_shared_by_clones() {
    let (client, server) = test_client().await;
    mount_projects(&server, &["proj-1"], 1).await;

    let url = client.trace_url("abc123").await.unwrap();
    assert_eq!(
        url,
        format!("{}/project/proj-1/traces/abc123", server.uri())
    );

    let clone = client.clone();
    assert_eq!(
        clone.trace_url("def456").await.unwrap(),
        format!("{}/project/proj-1/traces/def456", server.uri())
    );
    // `expect(1)`: the second call (on the clone) reused the cached project ID.
}

#[tokio::test]
async fn trace_url_keeps_base_path_and_encodes_segments() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/langfuse/api/public/projects"))
        .respond_with(json(200, projects_body(&["proj 1"])))
        .expect(1)
        .mount(&server)
        .await;
    let client = client_with_base_url(format!("{}/langfuse/", server.uri()));

    let url = client.trace_url("a/b").await.unwrap();
    assert_eq!(
        url,
        format!("{}/langfuse/project/proj%201/traces/a%2Fb", server.uri())
    );
}

#[tokio::test]
async fn requests_keep_the_base_url_path_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/langfuse/api/public/health"))
        .respond_with(json(200, json!({"version": "3.0.0", "status": "OK"})))
        .expect(2)
        .mount(&server)
        .await;

    // With and without a trailing slash.
    for base in [
        format!("{}/langfuse", server.uri()),
        format!("{}/langfuse/", server.uri()),
    ] {
        let client = client_with_base_url(base);
        assert_eq!(client.health.check().await.unwrap().status, "OK");
    }
}

#[tokio::test]
async fn auth_check_fills_the_trace_url_cache() {
    let (client, server) = test_client().await;
    mount_projects(&server, &["proj-1"], 1).await;

    client.auth_check().await.unwrap();
    let url = client.trace_url("t").await.unwrap();
    assert!(url.ends_with("/project/proj-1/traces/t"), "{url}");
}

// ═══════════════════════════════════════════════════════════════════════
//  Clone
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn clones_can_be_moved_into_tasks() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(200, json!({"version": "3.0.0", "status": "OK"})))
        .expect(3)
        .mount(&server)
        .await;

    let tasks: Vec<_> = (0..3)
        .map(|_| {
            let client = client.clone();
            tokio::spawn(async move { client.health.check().await })
        })
        .collect();
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap().status, "OK");
    }
}

#[test]
fn sub_clients_are_clone() {
    let config = ClientConfig::builder("pk", "sk").build().unwrap();
    let client = LangfuseClient::new(config).unwrap();
    let _datasets = client.datasets.clone();
    let _observations = client.observations.clone();
}

// ═══════════════════════════════════════════════════════════════════════
//  Custom reqwest::Client
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn custom_http_client_keeps_its_headers_and_gets_sdk_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .and(header("x-from-custom-client", "yes"))
        .and(header("x-langfuse-sdk-name", "rust"))
        .and(header("x-langfuse-public-key", PUBLIC_KEY))
        .and(header("authorization", "Basic cGstdGVzdDpzay10ZXN0"))
        .respond_with(json(200, json!({"version": "3.0.0", "status": "OK"})))
        .expect(1)
        .mount(&server)
        .await;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-from-custom-client", "yes".parse().unwrap());
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .http_client(http)
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();

    assert_eq!(client.health.check().await.unwrap().status, "OK");
}

async fn slow_health_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"version": "3.0.0", "status": "OK"}))
                .set_delay(Duration::from_millis(500)),
        )
        .mount(&server)
        .await;
    server
}

/// Timeouts of GET requests are retried; these tests check the timeout alone.
fn no_retries() -> RetryPolicy {
    RetryPolicy {
        max_retries: 0,
        ..Default::default()
    }
}

fn is_timeout(err: &LangfuseError) -> bool {
    matches!(err, LangfuseError::Transport(e) if e.is_timeout())
}

#[tokio::test]
async fn explicit_timeout_applies_to_a_custom_http_client() {
    let server = slow_health_server().await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .http_client(reqwest::Client::new())
        .timeout(Duration::from_millis(50))
        .retry_policy(no_retries())
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();

    let err = client.health.check().await.unwrap_err();
    assert!(is_timeout(&err), "{err:?}");
}

#[tokio::test]
async fn custom_http_client_keeps_its_own_timeout() {
    let server = slow_health_server().await;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_millis(50))
        .build()
        .unwrap();
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .http_client(http)
        .retry_policy(no_retries())
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();

    let err = client.health.check().await.unwrap_err();
    assert!(is_timeout(&err), "{err:?}");
}
