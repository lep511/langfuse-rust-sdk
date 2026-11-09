mod common;

use base64::Engine;
use common::*;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{header, method, path};

#[tokio::test]
async fn health_check_sends_auth_and_sdk_headers() {
    let (client, server) = test_client().await;
    let expected_auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{PUBLIC_KEY}:{SECRET_KEY}"))
    );
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .and(header("authorization", expected_auth.as_str()))
        .and(header("x-langfuse-sdk-name", "rust"))
        .and(header("x-langfuse-sdk-version", env!("CARGO_PKG_VERSION")))
        .and(header("x-langfuse-public-key", PUBLIC_KEY))
        .respond_with(json(200, json!({"status": "OK", "version": "3.1.0"})))
        .expect(1)
        .mount(&server)
        .await;

    let health = client.health.check().await.unwrap();
    assert_eq!(health.status, "OK");
    assert_eq!(health.version, "3.1.0");
}

#[tokio::test]
async fn health_check_503_maps_to_service_unavailable() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(503, json!({"message": "Database down"})))
        .mount(&server)
        .await;

    let err = client.health.check().await.unwrap_err();
    assert!(
        matches!(err, langfuse_rust_sdk::LangfuseError::ServiceUnavailable { ref message, .. } if message == "Database down"),
        "got {err:?}"
    );
}
