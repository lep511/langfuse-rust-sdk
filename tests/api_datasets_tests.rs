mod common;

use common::*;
use langfuse_rust_sdk::models::datasets::CreateDatasetRequest;
use langfuse_rust_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

fn dataset_json(name: &str) -> Value {
    json!({
        "id": "ds-1",
        "name": name,
        "description": null,
        "metadata": null,
        "inputSchema": null,
        "expectedOutputSchema": null,
        "projectId": "proj-1",
        "createdAt": TS,
        "updatedAt": TS
    })
}

#[tokio::test]
async fn list_datasets_sends_page_params() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets"))
        .and(query_param("page", "2"))
        .and(query_param("limit", "10"))
        .respond_with(json(
            200,
            json!({"data": [dataset_json("a")], "meta": {"page": 2, "limit": 10, "totalItems": 11, "totalPages": 2}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .datasets
        .list(&PageParams {
            page: Some(2),
            limit: Some(10),
        })
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.meta.total_items, 11);
}

#[tokio::test]
async fn create_dataset_posts_body() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/v2/datasets"))
        .and(body_json(json!({"name": "eval-set", "description": "d"})))
        .respond_with(json(200, dataset_json("eval-set")))
        .expect(1)
        .mount(&server)
        .await;

    let ds = client
        .datasets
        .create(&CreateDatasetRequest {
            name: "eval-set".into(),
            description: Some("d".into()),
            metadata: None,
            input_schema: None,
            expected_output_schema: None,
        })
        .await
        .unwrap();
    assert_eq!(ds.name, "eval-set");
}

#[tokio::test]
async fn get_dataset_encodes_name_with_spaces_and_slashes() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets/my%20folder%2Fset"))
        .respond_with(json(200, dataset_json("my folder/set")))
        .expect(1)
        .mount(&server)
        .await;

    let ds = client.datasets.get("my folder/set").await.unwrap();
    assert_eq!(ds.name, "my folder/set");
}
