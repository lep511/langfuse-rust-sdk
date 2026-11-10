//! `LangfuseClient::with_options`: per-view timeout, retries, headers, query
//! and body parameters, and the response hook.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use langfuse_rust_sdk::models::datasets::CreateDatasetRequest;
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError, RequestOptions};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dataset_json(name: &str) -> Value {
    json!({
        "id": "ds-1", "name": name, "description": null, "metadata": null,
        "inputSchema": null, "expectedOutputSchema": null, "projectId": "proj-1",
        "createdAt": TS, "updatedAt": TS
    })
}

async fn mock_health(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(response)
        .mount(server)
        .await;
}

fn healthy() -> ResponseTemplate {
    json(200, json!({"status": "OK", "version": "3.1.0"}))
}

#[tokio::test]
async fn timeout_applies_to_the_view_only() {
    let (client, server) = test_client().await;
    mock_health(&server, healthy().set_delay(Duration::from_millis(300))).await;

    let fast =
        client.with_options(RequestOptions::default().with_timeout(Duration::from_millis(50)));
    match fast.health.check().await {
        Err(LangfuseError::Transport(e)) => assert!(e.is_timeout(), "{e}"),
        other => panic!("expected a timeout, got {other:?}"),
    }
    // The original client keeps its own timeout.
    client.health.check().await.unwrap();
}

#[tokio::test]
async fn max_retries_overrides_the_policy_but_keeps_its_rules() {
    let (client, server) = test_client_with_retries(2).await;
    mock_health(&server, json(503, json!({"message": "down"}))).await;

    let once = client.with_options(RequestOptions::default().with_max_retries(0));
    assert!(matches!(
        once.health.check().await,
        Err(LangfuseError::ServiceUnavailable { .. })
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    client.health.check().await.unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 4);

    // More retries still never repeat a non-idempotent POST on a 500.
    Mock::given(method("POST"))
        .and(path("/api/public/v2/datasets"))
        .respond_with(json(500, json!({"message": "boom"})))
        .mount(&server)
        .await;
    let many = client.with_options(RequestOptions::default().with_max_retries(5));
    many.datasets
        .create(&CreateDatasetRequest {
            name: "d".into(),
            description: None,
            metadata: None,
            input_schema: None,
            expected_output_schema: None,
        })
        .await
        .unwrap_err();
    let posts = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST")
        .count();
    assert_eq!(posts, 1);
}

#[tokio::test]
async fn headers_are_added_and_replace_default_ones() {
    let server = MockServer::start().await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .header("x-team", "search")
        .build()
        .unwrap();
    let client = LangfuseClient::new(config).unwrap();
    mock_health(&server, healthy()).await;

    let view = client.with_options(
        RequestOptions::default()
            .with_header("x-team", "ranking")
            .with_header("x-request-source", "batch"),
    );
    view.health.check().await.unwrap();
    client.health.check().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let values = |i: usize, name: &str| -> Vec<String> {
        requests[i]
            .headers
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(values(0, "x-team"), ["ranking"]);
    assert_eq!(values(0, "x-request-source"), ["batch"]);
    assert_eq!(values(0, "x-langfuse-public-key"), [PUBLIC_KEY]);
    assert!(requests[0].headers.contains_key("authorization"));
    assert_eq!(values(1, "x-team"), ["search"]);
    assert!(values(1, "x-request-source").is_empty());
}

#[tokio::test]
async fn query_parameters_are_added_and_replace_same_named_ones() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets"))
        .respond_with(json(
            200,
            json!({"data": [dataset_json("d")], "meta": {"page": 3, "limit": 1, "totalItems": 3, "totalPages": 3}}),
        ))
        .mount(&server)
        .await;

    let view = client.with_options(
        RequestOptions::default()
            .with_query_param("page", "3")
            .with_query_param("debug", "a b&c"),
    );
    let params = PageParams {
        page: Some(1),
        limit: Some(1),
    };
    view.datasets.list(&params).await.unwrap();
    let request = single_request(&server).await;
    assert_eq!(
        query_pairs(&request),
        [
            ("limit".to_string(), "1".to_string()),
            ("page".to_string(), "3".to_string()),
            ("debug".to_string(), "a b&c".to_string()),
        ]
    );
}

#[tokio::test]
async fn body_parameters_merge_into_json_objects_only() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path("/api/public/v2/datasets"))
        .respond_with(json(200, dataset_json("d")))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(202, json!({"message": "Accepted"})))
        .mount(&server)
        .await;

    let view = client.with_options(
        RequestOptions::default()
            .with_body_param("description", "from options")
            .with_body_param("extra", json!({"nested": true})),
    );
    view.datasets
        .create(&CreateDatasetRequest {
            name: "d".into(),
            description: Some("from request".into()),
            metadata: None,
            input_schema: None,
            expected_output_schema: None,
        })
        .await
        .unwrap();
    // A JSON array body is sent unchanged.
    let score = CreateScoreRequest {
        trace_id: Some("t".into()),
        ..CreateScoreRequest::new("quality", CreateScoreValue::Numeric(1.0))
    };
    view.scores.create(&[score]).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        body_json(&requests[0]),
        json!({"name": "d", "description": "from options", "extra": {"nested": true}})
    );
    assert_eq!(
        requests[0].headers.get("content-type").unwrap(),
        "application/json"
    );
    let scores = body_json(&requests[1]);
    assert!(scores.is_array(), "{scores}");
    assert!(scores[0].get("extra").is_none());
}

#[tokio::test]
async fn the_response_hook_sees_every_attempt() {
    let (client, server) = test_client_with_retries(1).await;
    Mock::given(method("GET"))
        .and(path("/api/public/health"))
        .respond_with(json(503, json!({})).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mock_health(
        &server,
        healthy().insert_header("x-ratelimit-remaining", "41"),
    )
    .await;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let view = client.with_options(RequestOptions::default().with_response_hook(move |r| {
        let remaining = r
            .headers
            .get("x-ratelimit-remaining")
            .map(|v| v.to_str().unwrap().to_owned());
        sink.lock().unwrap().push((
            r.method.to_string(),
            r.url.path().to_owned(),
            r.status.as_u16(),
            r.attempt,
            remaining,
        ));
    }));
    view.health.check().await.unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        [
            ("GET".into(), "/api/public/health".into(), 503, 0, None),
            (
                "GET".into(),
                "/api/public/health".into(),
                200,
                1,
                Some("41".into())
            ),
        ]
    );
}

#[tokio::test]
async fn an_invalid_header_fails_every_request_without_sending_it() {
    let (client, server) = test_client().await;
    mock_health(&server, healthy()).await;
    let view = client.with_options(RequestOptions::default().with_header("bad header", "x"));
    match view.health.check().await {
        Err(LangfuseError::Config(message)) => assert!(message.contains("bad header"), "{message}"),
        other => panic!("expected a Config error, got {other:?}"),
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn views_nest_and_share_the_client_state() {
    let (client, server) = test_client().await;
    mock_health(&server, healthy()).await;
    Mock::given(method("GET"))
        .and(path("/api/public/projects"))
        .respond_with(json(
            200,
            json!({"data": [{"id": "proj-1", "name": "P", "metadata": {}, "organization": {"id": "o", "name": "O"}}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let outer = client.with_options(
        RequestOptions::default()
            .with_header("x-a", "outer")
            .with_header("x-b", "outer")
            .with_max_retries(0),
    );
    let inner = outer.with_options(RequestOptions::default().with_header("x-b", "inner"));
    assert_eq!(inner.request_options().max_retries, Some(0));
    assert!(client.request_options().headers.is_empty());
    inner.health.check().await.unwrap();
    let request = single_request(&server).await;
    assert_eq!(request.headers.get("x-a").unwrap(), "outer");
    assert_eq!(request.headers.get("x-b").unwrap(), "inner");

    // The cached project ID is shared: one auth check for both.
    inner.trace_url("t1").await.unwrap();
    client.trace_url("t2").await.unwrap();
}
