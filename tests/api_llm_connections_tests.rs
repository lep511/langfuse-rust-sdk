mod common;

use common::*;
use langfuse_sdk::models::llm_connections::{LlmAdapter, UpsertLlmConnectionRequest};
use langfuse_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

fn connection_json() -> Value {
    json!({
        "id": "llm-1",
        "provider": "my-gateway",
        "adapter": "openai",
        "displaySecretKey": "...abcd",
        "baseURL": "https://gateway.example.com/v1",
        "customModels": ["my-model"],
        "withDefaultModels": false,
        "extraHeaderKeys": ["x-team"],
        "config": {"useResponsesApi": true},
        "createdAt": TS,
        "updatedAt": TS
    })
}

fn page() -> Value {
    json!({"data": [connection_json()], "meta": {"page": 1, "limit": 10, "totalItems": 1, "totalPages": 1}})
}

fn upsert_request() -> UpsertLlmConnectionRequest {
    UpsertLlmConnectionRequest {
        base_url: Some("https://gateway.example.com/v1".into()),
        custom_models: Some(vec!["my-model".into()]),
        with_default_models: Some(false),
        extra_headers: Some(
            [("x-team".to_string(), "ml".to_string())]
                .into_iter()
                .collect(),
        ),
        config: Some(json!({"useResponsesApi": true})),
        ..UpsertLlmConnectionRequest::new("my-gateway", LlmAdapter::Openai, "sk-secret")
    }
}

fn expected_upsert_body() -> Value {
    json!({
        "provider": "my-gateway",
        "adapter": "openai",
        "secretKey": "sk-secret",
        "baseURL": "https://gateway.example.com/v1",
        "customModels": ["my-model"],
        "withDefaultModels": false,
        "extraHeaders": {"x-team": "ml"},
        "config": {"useResponsesApi": true}
    })
}

#[tokio::test]
async fn list_project_connections() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/llm-connections"))
        .and(query_param("page", "1"))
        .respond_with(json(200, page()))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .llm_connections
        .list(&PageParams {
            page: Some(1),
            limit: None,
        })
        .await
        .unwrap();
    let conn = &resp.data[0];
    assert_eq!(
        conn.base_url.as_deref(),
        Some("https://gateway.example.com/v1")
    );
    assert_eq!(conn.adapter, "openai");
    assert_eq!(conn.extra_header_keys, vec!["x-team"]);
}

#[tokio::test]
async fn upsert_project_connection_sends_base_url_key() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path("/api/public/llm-connections"))
        .and(body_json(expected_upsert_body()))
        .respond_with(json(200, connection_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .llm_connections
        .upsert(&upsert_request())
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_project_connection() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/llm-connections/llm-1"))
        .respond_with(json(200, json!({"message": "deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client
            .llm_connections
            .delete("llm-1")
            .await
            .unwrap()
            .message,
        "deleted"
    );
}

#[tokio::test]
async fn list_organization_connections() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/organizations/llm-connections"))
        .respond_with(json(200, page()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .llm_connections
        .list_organization(&PageParams::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn upsert_organization_connection() {
    let (client, server) = test_client().await;
    Mock::given(method("PUT"))
        .and(path("/api/public/organizations/llm-connections"))
        .and(body_json(expected_upsert_body()))
        .respond_with(json(200, connection_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .llm_connections
        .upsert_organization(&upsert_request())
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_organization_connection() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/organizations/llm-connections/llm-1"))
        .respond_with(json(200, json!({"message": "deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    client
        .llm_connections
        .delete_organization("llm-1")
        .await
        .unwrap();
}
