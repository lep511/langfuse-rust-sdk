mod common;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{method, path};

#[tokio::test]
async fn conflict_is_not_retried() {
    let (client, server) = test_client_with_retries(2).await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(409, json!({"message": "conflict"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client.health.check().await.unwrap_err();
    assert!(matches!(err, LangfuseError::Conflict { .. }), "got {err:?}");
}

#[tokio::test]
async fn service_unavailable_is_retried_until_exhausted() {
    let (client, server) = test_client_with_retries(2).await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(503, json!({"message": "down"})))
        .expect(3)
        .mount(&server)
        .await;

    let err = client.health.check().await.unwrap_err();
    assert!(
        matches!(err, LangfuseError::ServiceUnavailable { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn rate_limit_is_retried_then_succeeds() {
    let (client, server) = test_client_with_retries(2).await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(429, json!({"message": "slow"})).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(200, json!({"status": "OK", "version": "1"})))
        .expect(1)
        .mount(&server)
        .await;

    let health = client.health.check().await.unwrap();
    assert_eq!(health.status, "OK");
}

#[tokio::test]
async fn bad_request_is_not_retried() {
    let (client, server) = test_client_with_retries(2).await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(400, json!({"message": "bad"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client.health.check().await.unwrap_err();
    assert!(
        matches!(err, LangfuseError::BadRequest { .. }),
        "got {err:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Method-aware retries
// ═══════════════════════════════════════════════════════════════════════

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use langfuse_rust_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_rust_sdk::models::ingestion::IngestTracesRequest;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, RetryPolicy};
use wiremock::{MockServer, ResponseTemplate};

fn fast_retries(max_retries: u32) -> RetryPolicy {
    RetryPolicy {
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        max_retries,
        jitter_fraction: 0.0,
    }
}

fn client_for(base_url: String, policy: RetryPolicy, timeout: Option<Duration>) -> LangfuseClient {
    let mut builder = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(base_url)
        .retry_policy(policy);
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    LangfuseClient::new(builder.build().unwrap()).unwrap()
}

async fn count(server: &MockServer, http_method: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == http_method)
        .count()
}

#[tokio::test]
async fn post_server_error_is_not_retried() {
    let (client, server) = test_client_with_retries(3).await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(500, json!({"message": "boom"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client.scores.create(&[]).await.unwrap_err();
    assert!(
        matches!(err, LangfuseError::ServerError { status: 500, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn patch_server_error_is_not_retried() {
    let (client, server) = test_client_with_retries(3).await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/v2/prompts/p/versions/1"))
        .respond_with(json(502, json!({"message": "bad gateway"})))
        .expect(1)
        .mount(&server)
        .await;

    let req = UpdatePromptVersionRequest {
        new_labels: vec!["production".into()],
    };
    assert!(client.prompt_versions.update("p", 1, &req).await.is_err());
}

#[tokio::test]
async fn post_is_retried_when_the_request_was_not_processed() {
    for status in [408, 429, 503] {
        let (client, server) = test_client_with_retries(1).await;
        Mock::given(method("POST"))
            .and(path("/api/public/scores"))
            .respond_with(json(status, json!({"message": "try again"})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/public/scores"))
            .respond_with(json(202, json!({"message": "Accepted"})))
            .mount(&server)
            .await;

        client.scores.create(&[]).await.unwrap();
        assert_eq!(count(&server, "POST").await, 2, "status {status}");
    }
}

#[tokio::test]
async fn idempotent_server_errors_are_retried() {
    let (client, server) = test_client_with_retries(1).await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/traces/t1"))
        .respond_with(json(500, json!({"message": "boom"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/traces/t1"))
        .respond_with(json(200, json!({"message": "Trace deleted"})))
        .mount(&server)
        .await;

    client.traces.delete("t1").await.unwrap();
    assert_eq!(count(&server, "DELETE").await, 2);
}

#[tokio::test]
async fn trace_ingestion_is_retried_on_server_errors() {
    let (client, server) = test_client_with_retries(1).await;
    Mock::given(method("POST"))
        .and(path("/api/public/otel/v1/traces"))
        .respond_with(json(502, json!({"message": "bad gateway"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/public/otel/v1/traces"))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;

    let req = IngestTracesRequest {
        resource_spans: vec![],
    };
    client.ingestion.ingest_traces(&req).await.unwrap();
    assert_eq!(count(&server, "POST").await, 2);
}

#[tokio::test]
async fn timeouts_are_retried_only_for_idempotent_requests() {
    let server = MockServer::start().await;
    let slow = |body| {
        ResponseTemplate::new(200)
            .set_body_json(body)
            .set_delay(Duration::from_millis(300))
    };
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(slow(json!({"version": "3.0.0", "status": "OK"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(slow(json!({"message": "Accepted"})))
        .mount(&server)
        .await;
    let client = client_for(
        server.uri(),
        fast_retries(2),
        Some(Duration::from_millis(50)),
    );

    let err = client.health.check().await.unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Transport(e) if e.is_timeout()),
        "{err:?}"
    );
    assert_eq!(count(&server, "GET").await, 3);

    let err = client.scores.create(&[]).await.unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Transport(e) if e.is_timeout()),
        "{err:?}"
    );
    assert_eq!(
        count(&server, "POST").await,
        1,
        "a POST that may have been processed is not repeated"
    );
}

#[tokio::test]
async fn connection_failures_are_retried_even_for_post() {
    // Reserve a port and close it: the first attempt is refused.
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let policy = RetryPolicy {
        initial_backoff: Duration::from_millis(300),
        max_backoff: Duration::from_millis(300),
        max_retries: 2,
        jitter_fraction: 0.0,
    };
    let client = client_for(format!("http://{addr}"), policy, None);

    // The server comes up during the backoff before the retry.
    let server = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let listener = std::net::TcpListener::bind(addr).unwrap();
        let server = MockServer::builder().listener(listener).start().await;
        Mock::given(method("POST"))
            .and(path("/api/public/scores"))
            .respond_with(json(202, json!({"message": "Accepted"})))
            .mount(&server)
            .await;
        server
    });

    client.scores.create(&[]).await.unwrap();
    let server = server.await.unwrap();
    assert_eq!(
        count(&server, "POST").await,
        1,
        "the refused attempt never reached it"
    );
}

/// A server that accepts each connection, reads the request and hangs up
/// without answering. Returns its URL and the number of connections.
fn hang_up_server() -> (String, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&connections);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
        }
    });
    (url, connections)
}

#[tokio::test]
async fn dropped_connections_are_retried_only_for_idempotent_requests() {
    let (url, connections) = hang_up_server();
    let client = client_for(url, fast_retries(2), None);

    assert!(matches!(
        client.health.check().await,
        Err(LangfuseError::Transport(_))
    ));
    assert_eq!(connections.load(Ordering::SeqCst), 3);

    assert!(matches!(
        client.scores.create(&[]).await,
        Err(LangfuseError::Transport(_))
    ));
    assert_eq!(
        connections.load(Ordering::SeqCst),
        4,
        "the POST is sent once"
    );
}
