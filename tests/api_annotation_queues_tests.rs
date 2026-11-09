mod common;

use common::*;
use langfuse_sdk::api::annotation_queues::ListQueueItemsParams;
use langfuse_sdk::models::annotation_queues::{
    AnnotationQueueObjectType, AnnotationQueueStatus, CreateAnnotationQueueItemRequest,
    CreateAnnotationQueueRequest, UpdateAnnotationQueueItemRequest,
};
use langfuse_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

const BASE: &str = "/api/public/annotation-queues";

fn queue_json() -> Value {
    json!({
        "id": "q-1",
        "name": "Review",
        "description": null,
        "scoreConfigIds": ["cfg-1"],
        "createdAt": TS,
        "updatedAt": TS
    })
}

fn item_json(status: &str) -> Value {
    json!({
        "id": "i-1",
        "queueId": "q-1",
        "objectId": "t-1",
        "objectType": "TRACE",
        "status": status,
        "completedAt": null,
        "createdAt": TS,
        "updatedAt": TS
    })
}

fn page(data: Value) -> Value {
    json!({"data": [data], "meta": {"page": 1, "limit": 10, "totalItems": 1, "totalPages": 1}})
}

#[tokio::test]
async fn list_queues() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .and(query_param("limit", "10"))
        .respond_with(json(200, page(queue_json())))
        .expect(1)
        .mount(&server)
        .await;

    let queues = client
        .annotation_queues
        .list_queues(&PageParams {
            page: None,
            limit: Some(10),
        })
        .await
        .unwrap();
    assert_eq!(queues.data[0].score_config_ids, vec!["cfg-1"]);
}

#[tokio::test]
async fn create_queue() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(
            json!({"name": "Review", "scoreConfigIds": ["cfg-1"]}),
        ))
        .respond_with(json(200, queue_json()))
        .expect(1)
        .mount(&server)
        .await;

    let queue = client
        .annotation_queues
        .create_queue(&CreateAnnotationQueueRequest {
            name: "Review".into(),
            description: None,
            score_config_ids: vec!["cfg-1".into()],
        })
        .await
        .unwrap();
    assert!(queue.description.is_none());
}

#[tokio::test]
async fn get_queue() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/q-1")))
        .respond_with(json(200, queue_json()))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        client
            .annotation_queues
            .get_queue("q-1")
            .await
            .unwrap()
            .name,
        "Review"
    );
}

#[tokio::test]
async fn list_items_filters_by_status() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/q-1/items")))
        .and(query_param("status", "PENDING"))
        .respond_with(json(200, page(item_json("PENDING"))))
        .expect(1)
        .mount(&server)
        .await;

    let items = client
        .annotation_queues
        .list_items(
            "q-1",
            &ListQueueItemsParams {
                status: Some(AnnotationQueueStatus::Pending),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(items.data[0].object_type, AnnotationQueueObjectType::Trace);
    assert_eq!(items.data[0].status, AnnotationQueueStatus::Pending);
}

#[tokio::test]
async fn create_item() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/q-1/items")))
        .and(body_json(json!({"objectId": "t-1", "objectType": "TRACE"})))
        .respond_with(json(200, item_json("PENDING")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .annotation_queues
        .create_item(
            "q-1",
            &CreateAnnotationQueueItemRequest {
                object_id: "t-1".into(),
                object_type: AnnotationQueueObjectType::Trace,
                status: None,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn get_item() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/q-1/items/i-1")))
        .respond_with(json(200, item_json("PENDING")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .annotation_queues
        .get_item("q-1", "i-1")
        .await
        .unwrap();
}

#[tokio::test]
async fn update_item_status() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/q-1/items/i-1")))
        .and(body_json(json!({"status": "COMPLETED"})))
        .respond_with(json(200, item_json("COMPLETED")))
        .expect(1)
        .mount(&server)
        .await;

    let item = client
        .annotation_queues
        .update_item(
            "q-1",
            "i-1",
            &UpdateAnnotationQueueItemRequest {
                status: Some(AnnotationQueueStatus::Completed),
            },
        )
        .await
        .unwrap();
    assert_eq!(item.status, AnnotationQueueStatus::Completed);
}

#[tokio::test]
async fn delete_item() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/q-1/items/i-1")))
        .respond_with(json(200, json!({"success": true, "message": "removed"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .annotation_queues
        .delete_item("q-1", "i-1")
        .await
        .unwrap();
    assert!(resp.success);
    assert_eq!(resp.message, "removed");
}

#[tokio::test]
async fn create_assignment() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/q-1/assignments")))
        .and(body_json(json!({"userId": "u-1"})))
        .respond_with(json(
            200,
            json!({"userId": "u-1", "queueId": "q-1", "projectId": "p"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .annotation_queues
        .create_assignment("q-1", "u-1")
        .await
        .unwrap();
    assert_eq!(resp.project_id, "p");
}

#[tokio::test]
async fn delete_assignment_sends_body() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/q-1/assignments")))
        .and(body_json(json!({"userId": "u-1"})))
        .respond_with(json(200, json!({"success": true})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .annotation_queues
        .delete_assignment("q-1", "u-1")
        .await
        .unwrap();
    assert!(resp.success);
}
