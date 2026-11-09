//! `LangfuseMedia` uploads, downloads and reference resolution.
mod common;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::media::{
    LangfuseMedia, MediaField, MediaSource, MediaTarget, MediaUploadStatus,
};
use langfuse_rust_sdk::models::media::MediaContentType;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn png() -> LangfuseMedia {
    LangfuseMedia::from_bytes(
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
        MediaContentType::ImagePng,
    )
}

/// `POST /api/public/media` answering with an upload URL on the mock server.
async fn mount_upload_url(server: &MockServer, media: &LangfuseMedia, upload_path: &str) {
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(
            200,
            json!({"uploadUrl": format!("{}{upload_path}", server.uri()), "mediaId": media.media_id()}),
        ))
        .expect(1)
        .mount(server)
        .await;
}

async fn mount_patch(server: &MockServer, media: &LangfuseMedia) {
    Mock::given(method("PATCH"))
        .and(path(format!("/api/public/media/{}", media.media_id())))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(server)
        .await;
}

async fn patch_body(server: &MockServer) -> serde_json::Value {
    let requests = server.received_requests().await.unwrap();
    let patch = requests
        .iter()
        .find(|r| r.method.as_str() == "PATCH")
        .expect("PATCH sent");
    serde_json::from_slice(&patch.body).unwrap()
}

#[tokio::test]
async fn upload_requests_a_url_puts_the_bytes_and_reports_the_outcome() {
    let (client, server) = test_client().await;
    let media = png();
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .and(body_partial_json(json!({
            "traceId": "trace-1",
            "observationId": "obs-1",
            "contentType": "image/png",
            "contentLength": media.content_length(),
            "sha256Hash": media.sha256_hash(),
            "field": "input"
        })))
        .respond_with(json(
            200,
            json!({"uploadUrl": format!("{}/storage/bucket/key?sig=abc", server.uri()), "mediaId": media.media_id()}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/storage/bucket/key"))
        .and(header("content-type", "image/png"))
        .and(header("x-amz-checksum-sha256", media.sha256_hash()))
        .and(header("x-ms-blob-type", "BlockBlob"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    mount_patch(&server, &media).await;

    let status = media
        .upload(
            &client,
            &MediaTarget::observation("trace-1", "obs-1", MediaField::Input),
        )
        .await
        .unwrap();
    assert_eq!(status, MediaUploadStatus::Uploaded);

    let requests = server.received_requests().await.unwrap();
    let put = requests
        .iter()
        .find(|r| r.method.as_str() == "PUT")
        .unwrap();
    assert_eq!(put.body, media.bytes());
    assert!(
        !put.headers.contains_key("authorization"),
        "storage gets no Langfuse credentials"
    );
    let patch = patch_body(&server).await;
    assert_eq!(patch["uploadHttpStatus"], 200);
    assert!(patch["uploadTimeMs"].is_number());
    assert!(patch["uploadedAt"].is_string());
    assert!(patch.get("uploadHttpError").is_none());
}

#[tokio::test]
async fn already_uploaded_media_is_not_uploaded_again() {
    let (client, server) = test_client().await;
    let media = png();
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(
            200,
            json!({"uploadUrl": null, "mediaId": media.media_id()}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let status = media
        .upload(&client, &MediaTarget::trace("trace-1", MediaField::Output))
        .await
        .unwrap();
    assert_eq!(status, MediaUploadStatus::AlreadyUploaded);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_media_id_mismatch_cancels_the_upload() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .respond_with(json(
            200,
            json!({"uploadUrl": format!("{}/storage/x", server.uri()), "mediaId": "something-else"}),
        ))
        .mount(&server)
        .await;

    let err = png()
        .upload(&client, &MediaTarget::trace("trace-1", MediaField::Input))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Media(msg) if msg.contains("mismatch")),
        "{err:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1, "no PUT");
}

#[tokio::test]
async fn a_failed_storage_upload_is_reported_and_returned() {
    let (client, server) = test_client().await;
    let media = png();
    mount_upload_url(&server, &media, "/storage/x").await;
    Mock::given(method("PUT"))
        .and(path("/storage/x"))
        .respond_with(ResponseTemplate::new(403).set_body_string("SignatureDoesNotMatch"))
        .expect(1)
        .mount(&server)
        .await;
    mount_patch(&server, &media).await;

    let err = media
        .upload(&client, &MediaTarget::trace("trace-1", MediaField::Input))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Media(msg) if msg.contains("403")),
        "{err:?}"
    );
    let patch = patch_body(&server).await;
    assert_eq!(patch["uploadHttpStatus"], 403);
    assert_eq!(patch["uploadHttpError"], "SignatureDoesNotMatch");
}

#[tokio::test]
async fn storage_server_errors_are_retried() {
    let (client, server) = test_client_with_retries(1).await;
    let media = png();
    mount_upload_url(&server, &media, "/storage/x").await;
    Mock::given(method("PUT"))
        .and(path("/storage/x"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/storage/x"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    mount_patch(&server, &media).await;

    let status = media
        .upload(&client, &MediaTarget::trace("trace-1", MediaField::Input))
        .await
        .unwrap();
    assert_eq!(status, MediaUploadStatus::Uploaded);
    let puts = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "PUT")
        .count();
    assert_eq!(puts, 2);
}

#[tokio::test]
async fn google_cloud_storage_urls_get_no_s3_headers() {
    let (client, server) = test_client().await;
    let media = png();
    mount_upload_url(&server, &media, "/storage.googleapis.com/bucket/key").await;
    Mock::given(method("PUT"))
        .and(path("/storage.googleapis.com/bucket/key"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    mount_patch(&server, &media).await;

    media
        .upload(&client, &MediaTarget::trace("trace-1", MediaField::Input))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let put = requests
        .iter()
        .find(|r| r.method.as_str() == "PUT")
        .unwrap();
    assert!(!put.headers.contains_key("x-amz-checksum-sha256"));
    assert!(!put.headers.contains_key("x-ms-blob-type"));
    assert_eq!(put.headers["content-type"], "image/png");
}

#[tokio::test]
async fn dataset_item_targets_are_sent() {
    let (client, server) = test_client().await;
    let media = png();
    Mock::given(method("POST"))
        .and(path("/api/public/media"))
        .and(body_partial_json(json!({
            "datasetId": "ds-1",
            "datasetItemId": "item-1",
            "field": "expectedOutput"
        })))
        .respond_with(json(
            200,
            json!({"uploadUrl": null, "mediaId": media.media_id()}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    media
        .upload(
            &client,
            &MediaTarget::dataset_item("ds-1", "item-1", MediaField::ExpectedOutput),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("traceId").is_none());
}

#[tokio::test]
async fn references_are_resolved_to_data_uris() {
    let (client, server) = test_client().await;
    let media = png();
    Mock::given(method("GET"))
        .and(path(format!("/api/public/media/{}", media.media_id())))
        .respond_with(json(
            200,
            json!({
                "mediaId": media.media_id(), "contentType": "image/png",
                "contentLength": media.content_length(), "uploadedAt": TS,
                "url": format!("{}/files/image", server.uri()), "urlExpiry": TS
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/image"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(media.bytes().to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/media/missing"))
        .respond_with(json(404, json!({"message": "Media not found"})))
        .mount(&server)
        .await;

    let reference = media.reference_string();
    let missing = "@@@langfuseMedia:type=image/png|id=missing|source=bytes@@@";
    let value = json!({
        "image": reference,
        "again": [format!("see {reference} above")],
        "missing": missing,
        "number": 3
    });
    let resolved = client.media.resolve_references(&value).await;
    assert_eq!(resolved["image"], media.to_data_uri());
    assert_eq!(
        resolved["again"][0],
        format!("see {} above", media.to_data_uri())
    );
    assert_eq!(resolved["missing"], missing, "unresolvable references stay");
    assert_eq!(resolved["number"], 3);
}

#[tokio::test]
async fn download_returns_metadata_and_bytes() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/media/m1"))
        .respond_with(json(
            200,
            json!({
                "mediaId": "m1", "contentType": "text/plain", "contentLength": 5, "uploadedAt": TS,
                "url": format!("{}/files/m1", server.uri()), "urlExpiry": TS
            }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/m1"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .mount(&server)
        .await;

    let downloaded = client.media.download("m1").await.unwrap();
    assert_eq!(downloaded.bytes, b"hello");
    assert_eq!(downloaded.media.content_type, "text/plain");
    assert_eq!(downloaded.to_data_uri(), "data:text/plain;base64,aGVsbG8=");
    let requests = server.received_requests().await.unwrap();
    let file = requests
        .iter()
        .find(|r| r.url.path() == "/files/m1")
        .unwrap();
    assert!(
        !file.headers.contains_key("authorization"),
        "signed URLs get no Langfuse credentials"
    );
}

#[tokio::test]
async fn download_fails_on_a_storage_error() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/media/m1"))
        .respond_with(json(
            200,
            json!({
                "mediaId": "m1", "contentType": "image/png", "contentLength": 1, "uploadedAt": TS,
                "url": format!("{}/files/gone", server.uri()), "urlExpiry": TS
            }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/gone"))
        .respond_with(ResponseTemplate::new(410))
        .mount(&server)
        .await;

    let err = client.media.download("m1").await.unwrap_err();
    assert!(
        matches!(&err, LangfuseError::Media(msg) if msg.contains("410")),
        "{err:?}"
    );
}

#[test]
fn media_from_a_file() {
    let dir = std::env::temp_dir().join(format!("langfuse-media-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("note.txt");
    std::fs::write(&file, b"hello").unwrap();

    let media = LangfuseMedia::from_file(&file, MediaContentType::TextPlain).unwrap();
    assert_eq!(media.source(), MediaSource::File);
    assert_eq!(media.bytes(), b"hello");
    assert!(media.reference_string().ends_with("|source=file@@@"));
    std::fs::remove_dir_all(&dir).unwrap();

    let missing = LangfuseMedia::from_file(dir.join("missing.txt"), MediaContentType::TextPlain);
    assert!(matches!(missing, Err(LangfuseError::Media(_))));
}

#[test]
fn content_types_map_to_mime_strings() {
    assert_eq!(
        MediaContentType::from_mime("IMAGE/PNG"),
        Some(MediaContentType::ImagePng)
    );
    assert_eq!(MediaContentType::ImagePng.mime(), "image/png");
    assert_eq!(MediaContentType::from_mime("application/x-unknown"), None);
}
