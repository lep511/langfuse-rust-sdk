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
