mod common;

use common::*;
use langfuse_sdk::api::dataset_items::ListDatasetItemsParams;
use langfuse_sdk::models::dataset_items::{
    CreateDatasetItemRequest, DatasetItemMediaReferenceField,
};
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

fn item_json() -> Value {
    json!({
        "id": "item-1",
        "status": "ACTIVE",
        "input": {"image": "@@@langfuseMedia:type=image/png|id=m-1|source=bytes@@@"},
        "expectedOutput": null,
        "metadata": null,
        "sourceTraceId": null,
        "sourceObservationId": null,
        "datasetId": "ds-1",
        "datasetName": "eval",
        "createdAt": TS,
        "updatedAt": TS,
        "version": TS,
        "mediaReferences": [{
            "field": "input",
            "referenceString": "@@@langfuseMedia:type=image/png|id=m-1|source=bytes@@@",
            "jsonPath": "$['image']",
            "media": {
                "mediaId": "m-1",
                "contentType": "image/png",
                "contentLength": 10,
                "url": "https://example.com/m-1",
                "urlExpiry": "2024-06-15T11:30:00Z"
            }
        }]
    })
}

#[tokio::test]
async fn list_items_with_version() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/dataset-items"))
        .and(query_param("datasetName", "eval"))
        .and(query_param("version", TS))
        .respond_with(json(
            200,
            json!({"data": [item_json()], "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .dataset_items
        .list(&ListDatasetItemsParams {
            dataset_name: Some("eval".into()),
            version: Some(TS.into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let item = &page.data[0];
    assert!(item.version.is_some());
    assert_eq!(item.media_references.len(), 1);
    let reference = &item.media_references[0];
    assert_eq!(reference.field, DatasetItemMediaReferenceField::Input);
    assert_eq!(reference.json_path, "$['image']");
    assert_eq!(reference.media.media_id, "m-1");
}

#[tokio::test]
async fn create_and_get_item() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/dataset-items"))
        .and(body_json(json!({"datasetName": "eval", "input": {"q": 1}})))
        .respond_with(json(200, item_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/dataset-items/item-1"))
        .respond_with(json(200, item_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .dataset_items
        .create(&CreateDatasetItemRequest {
            dataset_name: "eval".into(),
            input: Some(json!({"q": 1})),
            expected_output: None,
            metadata: None,
            source_trace_id: None,
            source_observation_id: None,
            id: None,
            status: None,
        })
        .await
        .unwrap();
    let item = client.dataset_items.get("item-1").await.unwrap();
    assert_eq!(item.dataset_name, "eval");
}

#[tokio::test]
async fn delete_item_returns_message() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path("/api/public/dataset-items/item-1"))
        .respond_with(json(200, json!({"message": "Dataset item deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client.dataset_items.delete("item-1").await.unwrap();
    assert_eq!(resp.message, "Dataset item deleted");
}
