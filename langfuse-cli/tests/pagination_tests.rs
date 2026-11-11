//! `--all` / `--max-items` over page and cursor lists.

mod common;

use common::{api, json, query_pairs};
use langfuse_rust_cli::error::Exit;
use serde_json::json;
use wiremock::matchers::{method, query_param};
use wiremock::{Mock, MockServer};

async fn page_server(pages: u64, per_page: u64) -> MockServer {
    let server = MockServer::start().await;
    for page in 1..=pages {
        let data: Vec<_> = (0..per_page)
            .map(|i| json!({"id": format!("{page}-{i}")}))
            .collect();
        Mock::given(method("GET"))
            .and(query_param("page", page.to_string()))
            .respond_with(json(200, json!({"data": data, "meta": {"page": page, "limit": per_page, "totalItems": pages * per_page, "totalPages": pages}})))
            .mount(&server)
            .await;
    }
    server
}

#[tokio::test]
async fn page_lists_are_merged() {
    let server = page_server(3, 2).await;
    let output = api(&server, &["api", "models", "list", "--limit", "2", "--all"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let body = output.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 6);
    assert_eq!(
        body["meta"],
        json!({"pages": 3, "fetchedItems": 6, "totalItems": 6, "truncated": false})
    );
    let pages: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            query_pairs(request)
                .into_iter()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
        })
        .collect();
    assert_eq!(pages, ["1", "2", "3"]);
}

#[tokio::test]
async fn max_items_truncates_with_a_notice() {
    let server = page_server(3, 2).await;
    let output = api(
        &server,
        &["api", "models", "list", "--all", "--max-items", "3"],
    )
    .await;
    let body = output.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    assert_eq!(body["meta"]["truncated"], true);
    assert_eq!(
        output.stderr,
        "--all stopped at 3 item(s) (--max-items 3); more data is available\n"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn cursor_lists_follow_meta_cursor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("cursor", "c2"))
        .respond_with(json(200, json!({"data": [{"id": "b"}], "meta": {}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(json(
            200,
            json!({"data": [{"id": "a"}], "meta": {"cursor": "c2"}}),
        ))
        .mount(&server)
        .await;
    let output = api(&server, &["api", "scores", "list", "--all"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(
        output.json(),
        json!({"data": [{"id": "a"}, {"id": "b"}], "meta": {"pages": 2, "fetchedItems": 2, "truncated": false}})
    );

    let output = api(
        &server,
        &["api", "scores", "list", "--all", "--format", "ndjson"],
    )
    .await;
    assert_eq!(output.stdout, "{\"id\":\"a\"}\n{\"id\":\"b\"}\n");
}

#[tokio::test]
async fn an_error_after_the_first_page_aborts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("page", "2"))
        .respond_with(json(500, json!({"message": "boom"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(json(
            200,
            json!({"data": [{"id": 1}], "meta": {"page": 1, "totalPages": 5}}),
        ))
        .mount(&server)
        .await;
    let output = api(&server, &["api", "datasets", "list", "--all"]).await;
    assert_eq!(output.exit, Exit::Http);
    assert_eq!(
        output.stderr,
        "--all aborted after 1 page(s) and 1 item(s): HTTP 500\n"
    );
    assert_eq!(output.json()["message"], "boom");
}

#[tokio::test]
async fn requests_are_capped_at_100() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(
            200,
            json!({"data": [{"id": 1}], "meta": {"cursor": "next"}}),
        ))
        .mount(&server)
        .await;
    let output = api(
        &server,
        &[
            "api",
            "observations",
            "list",
            "--limit",
            "1",
            "--all",
            "--max-items",
            "5000",
        ],
    )
    .await;
    assert_eq!(output.exit, Exit::Success);
    assert_eq!(output.json()["meta"]["pages"], 100);
    assert_eq!(output.json()["meta"]["truncated"], true);
    assert!(
        output
            .stderr
            .starts_with("--all stopped after 100 requests with 100 item(s)")
    );
}
