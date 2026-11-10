mod common;

use std::time::Duration;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

async fn error_for(template: ResponseTemplate) -> LangfuseError {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(template)
        .mount(&server)
        .await;
    client.health.check().await.unwrap_err()
}

async fn error_for_status(status: u16) -> LangfuseError {
    error_for(json(status, json!({"message": format!("status {status}")}))).await
}

#[tokio::test]
async fn maps_400_to_bad_request() {
    let err = error_for_status(400).await;
    assert!(
        matches!(err, LangfuseError::BadRequest { ref message, ref body }
        if message == "status 400" && body.is_some()),
        "got {err:?}"
    );
}

#[tokio::test]
async fn maps_401_to_unauthorized() {
    let err = error_for_status(401).await;
    assert!(
        matches!(err, LangfuseError::Unauthorized { ref message, .. } if message == "status 401")
    );
}

#[tokio::test]
async fn maps_403_to_access_denied() {
    let err = error_for_status(403).await;
    assert!(
        matches!(err, LangfuseError::AccessDenied { ref message, .. } if message == "status 403")
    );
}

#[tokio::test]
async fn maps_404_to_not_found() {
    let err = error_for_status(404).await;
    assert!(matches!(err, LangfuseError::NotFound { ref message, .. } if message == "status 404"));
}

#[tokio::test]
async fn maps_405_to_method_not_allowed() {
    let err = error_for_status(405).await;
    assert!(
        matches!(err, LangfuseError::MethodNotAllowed { ref message, .. } if message == "status 405")
    );
}

#[tokio::test]
async fn maps_409_to_conflict() {
    let err = error_for_status(409).await;
    assert!(matches!(err, LangfuseError::Conflict { ref message, .. } if message == "status 409"));
}

#[tokio::test]
async fn maps_412_to_precondition_failed() {
    let err = error_for_status(412).await;
    assert!(
        matches!(err, LangfuseError::PreconditionFailed { ref message, .. } if message == "status 412")
    );
}

#[tokio::test]
async fn maps_500_to_server_error() {
    let err = error_for_status(500).await;
    assert!(
        matches!(err, LangfuseError::ServerError { status: 500, ref message, .. } if message == "status 500")
    );
}

#[tokio::test]
async fn maps_503_to_service_unavailable() {
    let err = error_for_status(503).await;
    assert!(
        matches!(err, LangfuseError::ServiceUnavailable { ref message, .. } if message == "status 503")
    );
}

#[tokio::test]
async fn maps_other_4xx_to_client_error() {
    for status in [402, 408, 413, 422] {
        let err = error_for_status(status).await;
        assert!(
            matches!(err, LangfuseError::ClientError { status: s, ref message, .. }
            if s == status && *message == format!("status {status}")),
            "{status}: got {err:?}"
        );
        assert_eq!(err.status(), Some(status));
        assert_eq!(
            err.to_string(),
            format!("Client error ({status}): status {status}")
        );
    }
}

#[tokio::test]
async fn status_and_message_accessors() {
    for status in [400, 401, 403, 404, 405, 409, 412, 429, 500, 502, 503] {
        let err = error_for_status(status).await;
        assert_eq!(err.status(), Some(status), "{err:?}");
        assert_eq!(err.message(), Some(format!("status {status}").as_str()));
    }
    let config_error = LangfuseError::Config("bad".into());
    assert_eq!(config_error.status(), None);
    assert_eq!(config_error.message(), None);
}

#[tokio::test]
async fn non_json_body_is_kept_in_the_message() {
    let err = error_for(ResponseTemplate::new(400).set_body_string("  <html>oops</html>\n")).await;
    assert!(
        matches!(err, LangfuseError::BadRequest { ref message, body: None } if message == "<html>oops</html>"),
        "got {err:?}"
    );
}

#[tokio::test]
async fn long_non_json_body_is_shortened() {
    let err = error_for(ResponseTemplate::new(502).set_body_string("é".repeat(600))).await;
    let message = err.message().unwrap();
    assert_eq!(
        message.chars().count(),
        501,
        "500 characters and an ellipsis"
    );
    assert!(message.ends_with('…'));
    assert_eq!(err.status(), Some(502));
}

#[tokio::test]
async fn empty_body_gives_the_status_reason() {
    let err = error_for(ResponseTemplate::new(404)).await;
    assert!(
        matches!(err, LangfuseError::NotFound { ref message, body: None } if message == "Not Found"),
        "got {err:?}"
    );
    let err = error_for(ResponseTemplate::new(422)).await;
    assert_eq!(err.message(), Some("Unprocessable Entity"));
}

#[tokio::test]
async fn json_body_with_error_field_gives_the_message() {
    let err = error_for(json(400, json!({"error": "invalid"}))).await;
    match err {
        LangfuseError::BadRequest { message, body } => {
            assert_eq!(message, "invalid");
            assert_eq!(body.unwrap()["error"], "invalid");
        }
        other => panic!("got {other:?}"),
    }
    let err = error_for(json(401, json!({"error": {"message": "bad key"}}))).await;
    assert_eq!(err.message(), Some("bad key"));
    // `message` wins over `error`.
    let err = error_for(json(
        400,
        json!({"message": "Invalid request data", "error": [{"path": ["name"]}]}),
    ))
    .await;
    assert_eq!(err.message(), Some("Invalid request data"));
}

#[tokio::test]
async fn json_body_without_a_message_is_the_message() {
    let err = error_for(json(400, json!({"code": 7}))).await;
    assert_eq!(err.message(), Some(r#"{"code":7}"#));
    assert_eq!(err.body().unwrap()["code"], 7);
}

#[tokio::test]
async fn rate_limited_reads_retry_after() {
    let err =
        error_for(json(429, json!({"message": "slow down"})).insert_header("Retry-After", "7"))
            .await;
    assert_eq!(
        err.to_string(),
        "Rate limited (429): slow down (retry after Some(7s))"
    );
    match err {
        LangfuseError::RateLimited {
            message,
            retry_after,
            body,
        } => {
            assert_eq!(message, "slow down");
            assert_eq!(retry_after, Some(Duration::from_secs(7)));
            assert_eq!(body.unwrap()["message"], "slow down");
        }
        other => panic!("got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_success_body_is_serialization_error() {
    let err = error_for(ResponseTemplate::new(200).set_body_string("not json")).await;
    assert!(
        matches!(err, LangfuseError::Serialization(_)),
        "got {err:?}"
    );
}
