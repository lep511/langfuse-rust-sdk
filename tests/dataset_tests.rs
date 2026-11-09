//! `LangfuseClient::get_dataset` and `create_dataset_item` (media upload).
mod common;

use chrono::{DateTime, Utc};
use common::*;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::api::datasets::GetDatasetOptions;
use langfuse_rust_sdk::media::LangfuseMedia;
use langfuse_rust_sdk::models::dataset_items::CreateDatasetItemRequest;
use langfuse_rust_sdk::models::media::MediaContentType;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dataset_json() -> Value {
    json!({
        "id": "ds-1",
        "name": "capitals/eu",
        "description": "Capital cities",
        "metadata": {"owner": "qa"},
        "inputSchema": null,
        "expectedOutputSchema": null,
        "projectId": "proj-1",
        "createdAt": TS,
        "updatedAt": TS
    })
}

fn item_json(id: &str, input: Value) -> Value {
    json!({
        "id": id,
        "status": "ACTIVE",
        "input": input,
        "expectedOutput": "Paris",
        "metadata": null,
        "sourceTraceId": null,
        "sourceObservationId": null,
        "datasetId": "ds-1",
        "datasetName": "capitals/eu",
        "createdAt": TS,
        "updatedAt": TS
    })
}

fn page(items: Vec<Value>, page: i64, total_pages: i64) -> Value {
    json!({"data": items, "meta": {"page": page, "limit": 2, "totalItems": 3, "totalPages": total_pages}})
}

async fn mount_dataset(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets/capitals%2Feu"))
        .respond_with(json(200, dataset_json()))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn get_dataset_fetches_every_page_of_items() {
    let (client, server) = test_client().await;
    mount_dataset(&server).await;
    for (number, items, total) in [
        (
            "1",
            vec![
                item_json("i-1", json!("France")),
                item_json("i-2", json!("Italy")),
            ],
            2,
        ),
        ("2", vec![item_json("i-3", json!("Spain"))], 2),
    ] {
        Mock::given(method("GET"))
            .and(path("/api/public/dataset-items"))
            .and(query_param("datasetName", "capitals/eu"))
            .and(query_param("limit", "2"))
            .and(query_param("page", number))
            .respond_with(json(200, page(items, number.parse().unwrap(), total)))
            .expect(1)
            .mount(&server)
            .await;
    }

    let dataset = client
        .get_dataset(
            "capitals/eu",
            &GetDatasetOptions::default().with_fetch_items_page_size(2),
        )
        .await
        .unwrap();
    assert_eq!(dataset.id, "ds-1");
    assert_eq!(dataset.name, "capitals/eu");
    assert_eq!(dataset.project_id, "proj-1");
    assert_eq!(dataset.description.as_deref(), Some("Capital cities"));
    assert_eq!(dataset.metadata, Some(json!({"owner": "qa"})));
    let ids: Vec<&str> = dataset.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["i-1", "i-2", "i-3"]);
    assert!(dataset.version.is_none());
}

#[tokio::test]
async fn get_dataset_reads_items_at_a_version() {
    let (client, server) = test_client().await;
    mount_dataset(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/public/dataset-items"))
        .and(query_param("version", "2024-06-15T10:30:00Z"))
        .and(query_param("limit", "50"))
        .respond_with(json(200, page(vec![item_json("i-1", json!("x"))], 1, 1)))
        .expect(1)
        .mount(&server)
        .await;

    let version: DateTime<Utc> = TS.parse().unwrap();
    let dataset = client
        .get_dataset(
            "capitals/eu",
            &GetDatasetOptions::default().with_version(version),
        )
        .await
        .unwrap();
    assert_eq!(dataset.version, Some(version));
    assert_eq!(dataset.items.len(), 1);
}

#[tokio::test]
async fn get_dataset_rejects_an_empty_page_size() {
    let (client, server) = test_client().await;
    let err = client
        .get_dataset(
            "x",
            &GetDatasetOptions::default().with_fetch_items_page_size(0),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Config(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn items_without_media_are_created_directly() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/dataset-items"))
        .respond_with(json(200, item_json("i-1", json!({"q": "France"}))))
        .expect(1)
        .mount(&server)
        .await;

    client
        .create_dataset_item(&CreateDatasetItemRequest {
            input: Some(json!({"q": "France"})),
            ..CreateDatasetItemRequest::new("capitals/eu")
        })
        .await
        .unwrap();
    let request = single_request(&server).await;
    assert_eq!(
        body_json(&request),
        json!({"datasetName": "capitals/eu", "input": {"q": "France"}})
    );
}

fn png() -> LangfuseMedia {
    LangfuseMedia::from_bytes(
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
        MediaContentType::ImagePng,
    )
}

async fn mount_upload(server: &MockServer, media: &LangfuseMedia, storage_status: u16) {
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(
            200,
            json!({"uploadUrl": format!("{}/storage/key", server.uri()), "mediaId": media.media_id()}),
        ))
        .mount(server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/storage/key"))
        .respond_with(ResponseTemplate::new(storage_status))
        .mount(server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/public/media/{}", media.media_id())))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
}

#[tokio::test]
async fn media_is_uploaded_and_replaced_by_references_before_the_item_is_created() {
    let (client, server) = test_client().await;
    let media = png();
    mount_dataset(&server).await;
    mount_upload(&server, &media, 200).await;
    Mock::given(method("POST"))
        .and(path("/api/public/dataset-items"))
        .respond_with(json(200, item_json("i-1", json!({}))))
        .expect(1)
        .mount(&server)
        .await;

    client
        .create_dataset_item(&CreateDatasetItemRequest {
            // The same file in two fields is uploaded once, for the first.
            input: Some(json!({"question": "What is it?", "image": media})),
            metadata: Some(json!({"copy": media.to_data_uri()})),
            ..CreateDatasetItemRequest::new("capitals/eu")
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let order: Vec<String> = requests
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect();
    assert_eq!(
        order,
        [
            "GET /api/public/v2/datasets/capitals%2Feu".to_string(),
            "POST /api/public/media".into(),
            "PUT /storage/key".into(),
            format!("PATCH /api/public/media/{}", media.media_id()),
            "POST /api/public/dataset-items".into(),
        ]
    );
    let upload = body_json(&requests[1]);
    let created = body_json(&requests[4]);
    let item_id = created["id"].as_str().expect("an ID is generated");
    assert_eq!(upload["datasetId"], "ds-1");
    assert_eq!(upload["datasetItemId"], item_id);
    assert_eq!(upload["field"], "input");
    assert!(upload.get("traceId").is_none());
    // `LangfuseMedia` serializes as a data URI, so the reference names that source.
    let reference = LangfuseMedia::from_data_uri(&media.to_data_uri())
        .unwrap()
        .reference_string();
    assert_eq!(created["input"]["image"], json!(reference));
    assert_eq!(created["input"]["question"], "What is it?");
    assert_eq!(created["metadata"]["copy"], json!(reference));
}

#[tokio::test]
async fn expected_output_media_keeps_the_given_item_id() {
    let (client, server) = test_client().await;
    let media = png();
    mount_dataset(&server).await;
    mount_upload(&server, &media, 200).await;
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .and(body_partial_json(
            json!({"datasetItemId": "my-item", "field": "expectedOutput"}),
        ))
        .respond_with(json(
            200,
            json!({"uploadUrl": null, "mediaId": media.media_id()}),
        ))
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/public/dataset-items"))
        .and(body_partial_json(json!({"id": "my-item"})))
        .respond_with(json(200, item_json("my-item", json!("q"))))
        .expect(1)
        .mount(&server)
        .await;

    client
        .create_dataset_item(&CreateDatasetItemRequest {
            id: Some("my-item".into()),
            input: Some(json!("q")),
            expected_output: Some(json!(media)),
            ..CreateDatasetItemRequest::new("capitals/eu")
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_failed_upload_does_not_create_the_item() {
    let (client, server) = test_client().await;
    let media = png();
    mount_dataset(&server).await;
    mount_upload(&server, &media, 403).await;

    let err = client
        .create_dataset_item(&CreateDatasetItemRequest {
            input: Some(json!({"image": media})),
            ..CreateDatasetItemRequest::new("capitals/eu")
        })
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Media(_)), "{err:?}");
    let requests = server.received_requests().await.unwrap();
    assert!(
        !requests
            .iter()
            .any(|r| r.url.path() == "/api/public/dataset-items")
    );
}
