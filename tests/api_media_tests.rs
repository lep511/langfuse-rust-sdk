mod common;

use common::*;
use langfuse_sdk::models::media::{GetMediaUploadUrlRequest, MediaContentType, PatchMediaBody};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn get_media() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/media/m-1"))
        .respond_with(json(
            200,
            json!({
                "mediaId": "m-1",
                "contentType": "image/png",
                "contentLength": 1024,
                "uploadedAt": TS,
                "url": "https://example.com/m-1",
                "urlExpiry": "in one hour"
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let media = client.media.get("m-1").await.unwrap();
    assert_eq!(media.content_length, 1024);
    assert_eq!(media.url_expiry, "in one hour");
}

#[tokio::test]
async fn patch_media_sends_body_and_accepts_204() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path("/api/public/media/m-1"))
        .and(body_json(json!({
            "uploadedAt": TS,
            "uploadHttpStatus": 200,
            "uploadTimeMs": 350
        })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client
        .media
        .patch(
            "m-1",
            &PatchMediaBody {
                uploaded_at: TS.parse().unwrap(),
                upload_http_status: 200,
                upload_http_error: None,
                upload_time_ms: Some(350),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn get_upload_url_for_trace() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .and(body_json(json!({
            "traceId": "t-1",
            "contentType": "image/svg+xml",
            "contentLength": 512,
            "sha256Hash": "abc=",
            "field": "input"
        })))
        .respond_with(json(
            200,
            json!({"uploadUrl": "https://s3.example.com/presigned", "mediaId": "m-2"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .media
        .get_upload_url(&GetMediaUploadUrlRequest {
            trace_id: Some("t-1".into()),
            ..GetMediaUploadUrlRequest::new(MediaContentType::ImageSvgXml, 512, "abc=", "input")
        })
        .await
        .unwrap();
    assert_eq!(resp.media_id, "m-2");
    assert_eq!(
        resp.upload_url.as_deref(),
        Some("https://s3.example.com/presigned")
    );
}

#[tokio::test]
async fn get_upload_url_already_uploaded_has_null_url() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(200, json!({"uploadUrl": null, "mediaId": "m-3"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .media
        .get_upload_url(&GetMediaUploadUrlRequest {
            dataset_id: Some("ds".into()),
            dataset_item_id: Some("item".into()),
            ..GetMediaUploadUrlRequest::new(
                MediaContentType::ApplicationPdf,
                1,
                "h",
                "expectedOutput",
            )
        })
        .await
        .unwrap();
    assert!(resp.upload_url.is_none());
}
