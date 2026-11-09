mod common;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::api::comments::ListCommentsParams;
use langfuse_rust_sdk::api::feedback::{FeedbackTargetType, SubmitFeedbackRequest};
use langfuse_rust_sdk::models::comments::{CommentObjectType, CreateCommentRequest};
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

fn comment_json() -> Value {
    json!({
        "id": "c-1",
        "projectId": "p",
        "createdAt": TS,
        "updatedAt": TS,
        "objectType": "TRACE",
        "objectId": "t-1",
        "content": "Looks wrong",
        "authorUserId": null
    })
}

#[tokio::test]
async fn create_comment() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/comments"))
        .and(body_json(json!({
            "projectId": "p",
            "objectType": "trace",
            "objectId": "t-1",
            "content": "Looks wrong",
            "authorUserId": "u-1"
        })))
        .respond_with(json(200, json!({"id": "c-1"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .comments
        .create(&CreateCommentRequest {
            author_user_id: Some("u-1".into()),
            ..CreateCommentRequest::new("p", "trace", "t-1", "Looks wrong")
        })
        .await
        .unwrap();
    assert_eq!(resp.id, "c-1");
}

#[tokio::test]
async fn list_comments_with_filters() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/comments"))
        .and(query_param("objectType", "TRACE"))
        .and(query_param("objectId", "t-1"))
        .and(query_param("page", "1"))
        .respond_with(json(
            200,
            json!({"data": [comment_json()], "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .comments
        .list(&ListCommentsParams {
            page: Some(1),
            object_type: Some("TRACE".into()),
            object_id: Some("t-1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].object_type, CommentObjectType::Trace);
    assert!(page.data[0].author_user_id.is_none());
}

#[tokio::test]
async fn get_comment_by_id() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/comments/c-1"))
        .respond_with(json(200, comment_json()))
        .expect(1)
        .mount(&server)
        .await;

    let comment = client.comments.get("c-1").await.unwrap();
    assert_eq!(comment.content, "Looks wrong");
}

#[tokio::test]
async fn submit_feedback_returns_201() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/feedback"))
        .and(body_json(json!({
            "targetType": "public-api",
            "target": "GET /api/public/v3/scores",
            "feedback": "Docs unclear",
            "goal": "Export scores"
        })))
        .respond_with(json(201, json!({"id": "fb-1"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .feedback
        .submit(&SubmitFeedbackRequest {
            target_type: FeedbackTargetType::PublicApi,
            target: "GET /api/public/v3/scores".into(),
            feedback: "Docs unclear".into(),
            goal: Some("Export scores".into()),
            reference_url: None,
        })
        .await
        .unwrap();
    assert_eq!(resp.id, "fb-1");
}

#[tokio::test]
async fn submit_feedback_409_is_conflict() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/feedback"))
        .respond_with(json(409, json!({"message": "Duplicate feedback"})))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .feedback
        .submit(&SubmitFeedbackRequest {
            target_type: FeedbackTargetType::Cli,
            target: "langfuse-cli".into(),
            feedback: "x".into(),
            goal: None,
            reference_url: None,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(err, LangfuseError::Conflict { ref message, .. } if message == "Duplicate feedback")
    );
}
