mod common;

use common::*;
use langfuse_rust_sdk::models::enums::ModelUsageUnit;
use langfuse_rust_sdk::models::models_api::CreateModelRequest;
use langfuse_rust_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn model_json() -> Value {
    json!({
        "id": "m-1",
        "modelName": "my-llm",
        "matchPattern": "(?i)^my-llm$",
        "startDate": "2025-01-01T00:00:00.000Z",
        "unit": "TOKENS",
        "inputPrice": 0.001,
        "outputPrice": 0.002,
        "totalPrice": null,
        "tokenizerId": null,
        "tokenizerConfig": null,
        "isLangfuseManaged": false,
        "createdAt": TS,
        "prices": {"input": {"price": 0.001}},
        "pricingTiers": []
    })
}

#[tokio::test]
async fn list_models_parses_iso_start_date() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/models"))
        .and(query_param("limit", "20"))
        .respond_with(json(
            200,
            json!({"data": [model_json()], "meta": {"page": 1, "limit": 20, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .models
        .list(&PageParams {
            page: None,
            limit: Some(20),
        })
        .await
        .unwrap();
    let start = page.data[0].start_date.unwrap();
    assert_eq!(start.to_rfc3339(), "2025-01-01T00:00:00+00:00");
}

#[tokio::test]
async fn create_get_upsert_delete_model() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/models"))
        .and(body_partial_json(
            json!({"modelName": "my-llm", "unit": "TOKENS"}),
        ))
        .respond_with(json(200, model_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/models/m-1"))
        .respond_with(json(200, model_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/public/models/m-1"))
        .and(body_partial_json(
            json!({"startDate": "2025-01-01T00:00:00Z"}),
        ))
        .respond_with(json(200, model_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/models/m-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let req = CreateModelRequest::new("my-llm", "(?i)^my-llm$", ModelUsageUnit::Tokens);
    client.models.create(&req).await.unwrap();
    client.models.get("m-1").await.unwrap();
    let upsert = CreateModelRequest {
        start_date: Some("2025-01-01T00:00:00Z".parse().unwrap()),
        ..req
    };
    client.models.update("m-1", &upsert).await.unwrap();
    client.models.delete("m-1").await.unwrap();
}
