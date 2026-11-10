//! Lazy page/item streams (`*_pages` methods and `Pages::items`).
mod common;

use common::*;
use futures::{StreamExt, TryStreamExt};
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::api::annotation_queues::ListQueueItemsParams;
use langfuse_rust_sdk::api::comments::ListCommentsParams;
use langfuse_rust_sdk::api::dataset_items::ListDatasetItemsParams;
use langfuse_rust_sdk::api::experiments::{ListExperimentItemsParams, ListExperimentsParams};
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::api::prompts::ListPromptsParams;
use langfuse_rust_sdk::api::scores::ListScoresParams;
use langfuse_rust_sdk::pagination::{CursorParams, PageParams, Pages};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer};

fn dataset_json(name: &str) -> Value {
    json!({
        "id": format!("id-{name}"), "name": name, "description": null, "metadata": null,
        "inputSchema": null, "expectedOutputSchema": null, "projectId": "proj-1",
        "createdAt": TS, "updatedAt": TS
    })
}

fn observation_json(id: &str) -> Value {
    json!({
        "id": id, "traceId": "t-1", "projectId": "p", "type": "SPAN",
        "startTime": TS, "level": "DEFAULT"
    })
}

fn page_body(page: i64, total_pages: i64, data: Vec<Value>) -> Value {
    json!({
        "data": data,
        "meta": {"page": page, "limit": 2, "totalItems": 5, "totalPages": total_pages}
    })
}

fn cursor_body(data: Vec<Value>, cursor: Option<&str>) -> Value {
    json!({"data": data, "meta": {"cursor": cursor}})
}

async fn mount_dataset_page(server: &MockServer, page: i64, total: i64, names: &[&str], n: u64) {
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets"))
        .and(query_param("page", page.to_string()))
        .and(query_param("limit", "2"))
        .respond_with(json(
            200,
            page_body(
                page,
                total,
                names.iter().map(|name| dataset_json(name)).collect(),
            ),
        ))
        .expect(n)
        .mount(server)
        .await;
}

fn limit_two() -> PageParams {
    PageParams {
        page: None,
        limit: Some(2),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Page-based
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn page_stream_walks_every_page() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 3, &["a", "b"], 1).await;
    mount_dataset_page(&server, 2, 3, &["c", "d"], 1).await;
    mount_dataset_page(&server, 3, 3, &["e"], 1).await;

    let pages: Vec<_> = client
        .datasets
        .list_pages(limit_two())
        .try_collect()
        .await
        .unwrap();
    let page_numbers: Vec<i64> = pages.iter().map(|p| p.meta.page).collect();
    assert_eq!(page_numbers, [1, 2, 3]);
}

#[tokio::test]
async fn items_flattens_pages_in_order() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 3, &["a", "b"], 1).await;
    mount_dataset_page(&server, 2, 3, &["c", "d"], 1).await;
    mount_dataset_page(&server, 3, 3, &["e"], 1).await;

    let names: Vec<String> = client
        .datasets
        .list_pages(limit_two())
        .items()
        .map_ok(|d| d.name)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(names, ["a", "b", "c", "d", "e"]);
}

#[tokio::test]
async fn page_stream_starts_at_requested_page() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 3, &["a", "b"], 0).await;
    mount_dataset_page(&server, 2, 3, &["c", "d"], 1).await;
    mount_dataset_page(&server, 3, 3, &["e"], 1).await;

    let params = PageParams {
        page: Some(2),
        limit: Some(2),
    };
    let count = client.datasets.list_pages(params).items().count().await;
    assert_eq!(count, 3);
}

#[tokio::test]
async fn page_stream_with_no_results_makes_one_request() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 0, &[], 1).await;

    let pages: Vec<_> = client
        .datasets
        .list_pages(limit_two())
        .try_collect()
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
    assert!(pages[0].data.is_empty());
}

#[tokio::test]
async fn take_on_items_does_not_fetch_further_pages() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 3, &["a", "b"], 1).await;
    mount_dataset_page(&server, 2, 3, &["c", "d"], 0).await;

    let names: Vec<String> = client
        .datasets
        .list_pages(limit_two())
        .items()
        .take(2)
        .map_ok(|d| d.name)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(names, ["a", "b"]);
    // `expect(0)` on page 2 is verified when the server drops.
}

#[tokio::test]
async fn error_ends_the_stream() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 3, &["a", "b"], 1).await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets"))
        .and(query_param("page", "2"))
        .respond_with(json(500, json!({"message": "boom"})))
        .expect(1)
        .mount(&server)
        .await;
    mount_dataset_page(&server, 3, 3, &["e"], 0).await;

    let mut pages = client.datasets.list_pages(limit_two());
    assert!(pages.next().await.unwrap().is_ok());
    assert!(matches!(
        pages.next().await,
        Some(Err(LangfuseError::ServerError { status: 500, .. }))
    ));
    assert!(pages.next().await.is_none());
}

#[tokio::test]
async fn page_stream_keeps_filters_on_every_request() {
    let (client, server) = test_client().await;
    for page in ["1", "2"] {
        Mock::given(method("GET"))
            .and(path("/api/public/dataset-items"))
            .and(query_param("datasetName", "eval set"))
            .and(query_param("page", page))
            .respond_with(json(200, page_body(page.parse().unwrap(), 2, vec![])))
            .expect(1)
            .mount(&server)
            .await;
    }

    let params = ListDatasetItemsParams {
        dataset_name: Some("eval set".into()),
        ..Default::default()
    };
    let pages: Vec<_> = client
        .dataset_items
        .list_pages(params)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════
//  Cursor-based
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cursor_stream_follows_cursors() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param("traceId", "t-1"))
        .and(query_param_is_missing("cursor"))
        .respond_with(json(
            200,
            cursor_body(
                vec![observation_json("o1"), observation_json("o2")],
                Some("c2"),
            ),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param("traceId", "t-1"))
        .and(query_param("cursor", "c2"))
        .respond_with(json(
            200,
            cursor_body(vec![observation_json("o3")], Some("c3")),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param("traceId", "t-1"))
        .and(query_param("cursor", "c3"))
        .respond_with(json(200, cursor_body(vec![observation_json("o4")], None)))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListObservationsParams {
        trace_id: Some("t-1".into()),
        ..Default::default()
    };
    let ids: Vec<String> = client
        .observations
        .list_pages(params)
        .items()
        .map_ok(|o| o.id)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(ids, ["o1", "o2", "o3", "o4"]);
}

#[tokio::test]
async fn empty_cursor_ends_the_stream() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .respond_with(json(
            200,
            cursor_body(vec![observation_json("o1")], Some("")),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let pages: Vec<_> = client
        .observations
        .list_pages(ListObservationsParams::default())
        .try_collect()
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
}

#[tokio::test]
async fn repeated_cursor_ends_the_stream() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param("cursor", "same"))
        .respond_with(json(
            200,
            cursor_body(vec![observation_json("o1")], Some("same")),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = ListObservationsParams {
        cursor: Some("same".into()),
        ..Default::default()
    };
    let count = client.observations.list_pages(params).count().await;
    assert_eq!(count, 1);
}

#[tokio::test]
async fn take_on_cursor_items_stops_requesting() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param_is_missing("cursor"))
        .respond_with(json(
            200,
            cursor_body(
                vec![observation_json("o1"), observation_json("o2")],
                Some("c2"),
            ),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .and(query_param("cursor", "c2"))
        .respond_with(json(200, cursor_body(vec![], None)))
        .expect(0)
        .mount(&server)
        .await;

    let first = client
        .observations
        .list_pages(ListObservationsParams::default())
        .items()
        .take(1)
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
}

#[tokio::test]
async fn streams_are_send_and_can_be_spawned() {
    let (client, server) = test_client().await;
    mount_dataset_page(&server, 1, 1, &["a"], 1).await;

    let task_client = client.clone();
    let names = tokio::spawn(async move {
        task_client
            .datasets
            .list_pages(limit_two())
            .items()
            .map_ok(|d| d.name)
            .try_collect::<Vec<_>>()
            .await
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(names, ["a"]);
}

// ═══════════════════════════════════════════════════════════════════════
//  Every `*_pages` method requests the right path and pages
// ═══════════════════════════════════════════════════════════════════════

/// Mount two empty pages (`totalPages: 2`) at `route`.
async fn mount_two_pages(server: &MockServer, route: &str) {
    for page in [1, 2] {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param("page", page.to_string()))
            .respond_with(json(200, page_body(page, 2, vec![])))
            .expect(1)
            .mount(server)
            .await;
    }
}

/// Mount two empty cursor pages at `route`: no cursor → `next` → end.
async fn mount_two_cursor_pages(server: &MockServer, route: &str) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param_is_missing("cursor"))
        .respond_with(json(200, cursor_body(vec![], Some("next"))))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param("cursor", "next"))
        .respond_with(json(200, cursor_body(vec![], None)))
        .expect(1)
        .mount(server)
        .await;
}

async fn assert_two_pages<P>(pages: Pages<'_, P>) {
    let pages: Vec<P> = pages.try_collect().await.unwrap();
    assert_eq!(pages.len(), 2);
}

#[tokio::test]
async fn every_page_based_method_paginates() {
    let (client, server) = test_client().await;
    let routes = [
        "/api/public/comments",
        "/api/public/dataset-items",
        "/api/public/v2/datasets",
        "/api/public/models",
        "/api/public/llm-connections",
        "/api/public/organizations/llm-connections",
        "/api/public/annotation-queues",
        "/api/public/annotation-queues/q1/items",
        "/api/public/v2/prompts",
        "/api/public/score-configs",
    ];
    for route in routes {
        mount_two_pages(&server, route).await;
    }

    let c = &client;
    assert_two_pages(c.comments.list_pages(ListCommentsParams::default())).await;
    assert_two_pages(
        c.dataset_items
            .list_pages(ListDatasetItemsParams::default()),
    )
    .await;
    assert_two_pages(c.datasets.list_pages(PageParams::default())).await;
    assert_two_pages(c.models.list_pages(PageParams::default())).await;
    assert_two_pages(c.llm_connections.list_pages(PageParams::default())).await;
    assert_two_pages(
        c.llm_connections
            .list_organization_pages(PageParams::default()),
    )
    .await;
    assert_two_pages(c.annotation_queues.list_queues_pages(PageParams::default())).await;
    assert_two_pages(
        c.annotation_queues
            .list_items_pages("q1", ListQueueItemsParams::default()),
    )
    .await;
    assert_two_pages(c.prompts.list_pages(ListPromptsParams::default())).await;
    assert_two_pages(c.score_configs.list_pages(PageParams::default())).await;
}

#[cfg(feature = "unstable")]
#[tokio::test]
async fn every_unstable_page_based_method_paginates() {
    use langfuse_rust_sdk::api::unstable_topics::{ListTopicFacetsParams, ListTopicRulesParams};

    let (client, server) = test_client().await;
    for route in [
        "/api/public/unstable/dashboards",
        "/api/public/unstable/dashboard-widgets",
        "/api/public/unstable/topic-facets",
        "/api/public/unstable/topic-rules",
    ] {
        mount_two_pages(&server, route).await;
    }

    let c = &client;
    assert_two_pages(c.unstable_dashboards.list_pages(PageParams::default())).await;
    assert_two_pages(
        c.unstable_dashboard_widgets
            .list_pages(PageParams::default()),
    )
    .await;
    assert_two_pages(
        c.unstable_topics
            .list_facets_pages(ListTopicFacetsParams::default()),
    )
    .await;
    assert_two_pages(
        c.unstable_topics
            .list_rules_pages(ListTopicRulesParams::default()),
    )
    .await;
}

#[tokio::test]
async fn every_cursor_based_method_paginates() {
    let (client, server) = test_client().await;
    for route in [
        "/api/public/v2/observations",
        "/api/public/v3/scores",
        "/api/public/v2/evaluators",
        "/api/public/v2/evaluators/ev1/versions",
        "/api/public/experiments",
        "/api/public/experiment-items",
        "/api/public/v2/evaluation-rules",
    ] {
        mount_two_cursor_pages(&server, route).await;
    }

    let c = &client;
    assert_two_pages(c.observations.list_pages(ListObservationsParams::default())).await;
    assert_two_pages(c.scores.list_pages(ListScoresParams::default())).await;
    assert_two_pages(c.evaluators.list_pages(CursorParams::default())).await;
    assert_two_pages(
        c.evaluators
            .list_versions_pages("ev1", CursorParams::default()),
    )
    .await;
    assert_two_pages(c.experiments.list_pages(ListExperimentsParams::new(TS))).await;
    assert_two_pages(
        c.experiments
            .list_items_pages(ListExperimentItemsParams::new(TS)),
    )
    .await;
    assert_two_pages(c.evaluation_rules.list_pages(CursorParams::default())).await;
}

// ═══════════════════════════════════════════════════════════════════════
//  Page timeout of observations
// ═══════════════════════════════════════════════════════════════════════

/// Client with a 200 ms timeout and no retries, against a server answering
/// the observations list after 600 ms.
async fn client_with_short_timeout_and_slow_observations()
-> (langfuse_rust_sdk::LangfuseClient, MockServer) {
    use langfuse_rust_sdk::{ClientConfig, LangfuseClient, RetryPolicy};
    use std::time::Duration;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/observations"))
        .respond_with(
            json(200, cursor_body(vec![observation_json("o1")], None))
                .set_delay(Duration::from_millis(600)),
        )
        .mount(&server)
        .await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .timeout(Duration::from_millis(200))
        .retry_policy(RetryPolicy {
            max_retries: 0,
            ..RetryPolicy::default()
        })
        .build()
        .unwrap();
    (LangfuseClient::new(config).unwrap(), server)
}

fn timed_out(err: &LangfuseError) -> bool {
    matches!(err, LangfuseError::Transport(e) if e.is_timeout())
}

#[tokio::test]
async fn observation_pages_use_a_60_s_timeout() {
    let (client, _server) = client_with_short_timeout_and_slow_observations().await;

    // A single `list` keeps the client timeout...
    let err = client
        .observations
        .list(&ListObservationsParams::default())
        .await
        .unwrap_err();
    assert!(timed_out(&err), "{err:?}");

    // ...while the page requests wait for the slow scan, as in the Python SDK.
    let ids: Vec<String> = client
        .observations
        .list_pages(ListObservationsParams::default())
        .items()
        .map_ok(|o| o.id)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(ids, ["o1"]);
}

#[tokio::test]
async fn view_timeout_replaces_the_page_timeout() {
    use langfuse_rust_sdk::RequestOptions;
    use std::time::Duration;

    let (client, _server) = client_with_short_timeout_and_slow_observations().await;
    let view =
        client.with_options(RequestOptions::default().with_timeout(Duration::from_millis(100)));
    let err = view
        .observations
        .list_pages(ListObservationsParams::default())
        .next()
        .await
        .unwrap()
        .unwrap_err();
    assert!(timed_out(&err), "{err:?}");
}
