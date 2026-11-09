mod common;

use common::*;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path};

#[tokio::test]
async fn delete_trace_uses_delete_method() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/traces/trace-1"))
        .respond_with(json(200, json!({"message": "Trace deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.traces.delete("trace-1").await.unwrap();
    assert_eq!(resp.message, "Trace deleted");
}

#[tokio::test]
async fn delete_trace_encodes_path_segment() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/traces/a%2Fb%20c"))
        .respond_with(json(200, json!({"message": "ok"})))
        .expect(1)
        .mount(&server)
        .await;

    client.traces.delete("a/b c").await.unwrap();
}

#[tokio::test]
async fn delete_batch_sends_trace_ids_and_parses_response() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/traces"))
        .and(body_json(json!({"traceIds": ["t1", "t2"]})))
        .respond_with(json(200, json!({"message": "2 traces deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .traces
        .delete_batch(&langfuse_sdk::models::traces::DeleteTracesRequest {
            trace_ids: vec!["t1".into(), "t2".into()],
        })
        .await
        .unwrap();
    assert_eq!(resp.message, "2 traces deleted");
}

#[tokio::test]
async fn delete_trace_get_is_not_sent() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .respond_with(json(200, json!({"message": "wrong"})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(json(200, json!({"message": "ok"})))
        .mount(&server)
        .await;

    client.traces.delete("trace-1").await.unwrap();
}
