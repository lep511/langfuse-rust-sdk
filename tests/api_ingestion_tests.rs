mod common;

use common::*;
use langfuse_sdk::models::ingestion::IngestTracesRequest;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{body_json, header, method, path};

#[tokio::test]
async fn ingest_traces_sends_ingestion_version_header() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/otel/v1/traces"))
        .and(header("x-langfuse-ingestion-version", "4"))
        .and(header("content-type", "application/json"))
        .and(header("x-langfuse-public-key", PUBLIC_KEY))
        .and(body_json(json!({"resourceSpans": []})))
        .respond_with(json(200, json!({})))
        .expect(1)
        .mount(&server)
        .await;

    client
        .ingestion
        .ingest_traces(&IngestTracesRequest {
            resource_spans: vec![],
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn ingestion_header_is_not_sent_on_other_requests() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(200, json!({"status": "OK", "version": "1"})))
        .mount(&server)
        .await;

    client.health.check().await.unwrap();
    let req = single_request(&server).await;
    assert!(req.headers.get("x-langfuse-ingestion-version").is_none());
}
