#![cfg(feature = "unstable")]

mod common;

use common::*;
use langfuse_sdk::api::unstable_topics::*;
use langfuse_sdk::models::unstable_topics::*;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{method, path};

const WINDOW: &str = "2024-06-08T10:30:00Z";

fn window() -> Value {
    json!({"from": WINDOW, "to": TS})
}

fn facet_ref() -> Value {
    json!({"id": "f-1", "name": "Issues", "version": 2})
}

fn topic_json() -> Value {
    json!({
        "id": "topic-1", "versionId": "tv-1", "name": "Login failures", "description": "Users cannot log in",
        "facet": facet_ref(), "ruleId": null, "runId": "run-1", "traceCount": 12
    })
}

#[tokio::test]
async fn list_facets_with_filter() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-facets"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "f-1", "name": "Issues", "description": "User issues", "isBuiltIn": true,
                    "active": true, "ruleIds": ["r-1"], "version": 2, "prompt": "Classify...",
                    "publishedRun": {"id": "run-1", "completedAt": TS, "topicCount": 8}
                }],
                "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let filter = r#"[{"type":"boolean","column":"isBuiltIn","operator":"=","value":true}]"#;
    let page = client
        .unstable_topics
        .list_facets(&ListTopicFacetsParams {
            filter: Some(filter.into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let facet = &page.data[0];
    assert!(facet.is_built_in);
    assert_eq!(facet.published_run.as_ref().unwrap().topic_count, 8);
    let req = single_request(&server).await;
    assert_eq!(query_value(&req, "filter"), filter);
}

#[tokio::test]
async fn get_facet() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-facets/f-1"))
        .respond_with(json(
            200,
            json!({
                "id": "f-1", "name": "Issues", "description": "", "isBuiltIn": false,
                "active": false, "ruleIds": [], "version": 1, "prompt": "p", "publishedRun": null
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let facet = client.unstable_topics.get_facet("f-1").await.unwrap();
    assert!(facet.published_run.is_none());
}

#[tokio::test]
async fn list_topics_with_window_and_order() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topics"))
        .respond_with(json(
            200,
            json!({
                "window": window(),
                "data": [topic_json()],
                "facets": [{
                    "facet": facet_ref(), "ruleId": null, "publishedRunId": "run-1",
                    "counts": {"assigned": 12, "outlier": 1, "awaitingMap": 0, "notApplicable": 3, "insufficientInput": 0}
                }],
                "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let list = client
        .unstable_topics
        .list(&ListTopicsParams {
            from_timestamp: Some(WINDOW.into()),
            to_timestamp: Some(TS.into()),
            order_by: Some(TopicsOrderBy::NameAsc),
            limit: Some(50),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(list.data[0].trace_count, 12);
    assert_eq!(list.facets[0].counts.not_applicable, 3);
    assert_eq!(list.window.to.to_rfc3339(), "2024-06-15T10:30:00+00:00");

    let req = single_request(&server).await;
    assert_eq!(query_value(&req, "orderBy"), "name.asc");
    assert_eq!(query_value(&req, "fromTimestamp"), WINDOW);
    assert_eq!(query_value(&req, "limit"), "50");
}

#[tokio::test]
async fn get_topic_flattens_topic_fields() {
    let (client, server) = test_client().await;
    let mut body = topic_json();
    body["inCurrentMap"] = json!(true);
    body["window"] = window();
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topics/topic-1"))
        .respond_with(json(200, body))
        .expect(1)
        .mount(&server)
        .await;

    let topic = client
        .unstable_topics
        .get(
            "topic-1",
            &GetTopicParams {
                from_timestamp: Some(WINDOW.into()),
                to_timestamp: None,
            },
        )
        .await
        .unwrap();
    assert!(topic.in_current_map);
    assert_eq!(topic.topic.name, "Login failures");
    assert_eq!(topic.topic.facet.version, 2);
    let req = single_request(&server).await;
    assert_eq!(
        query_pairs(&req),
        vec![("fromTimestamp".to_string(), WINDOW.to_string())]
    );
}

#[tokio::test]
async fn list_and_get_rules() {
    let (client, server) = test_client().await;
    let rule = json!({
        "id": "r-1", "name": "Prod traces",
        "filter": [{"type": "stringOptions", "column": "environment", "operator": "any of", "value": ["production"]}],
        "sampling": 0.25, "facetIds": ["f-1"], "updatedAt": TS
    });
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-rules"))
        .respond_with(json(
            200,
            json!({"data": [rule.clone()], "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-rules/r-1"))
        .respond_with(json(200, rule))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .unstable_topics
        .list_rules(&ListTopicRulesParams::default())
        .await
        .unwrap();
    assert_eq!(page.data[0].filter[0].column, "environment");
    let rule = client.unstable_topics.get_rule("r-1").await.unwrap();
    assert!((rule.sampling - 0.25).abs() < f64::EPSILON);
}

#[tokio::test]
async fn list_summaries_with_cursor() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-summaries"))
        .respond_with(json(
            200,
            json!({
                "window": window(),
                "data": [{
                    "traceId": "t-1", "traceName": "chat", "environment": "production", "timestamp": TS,
                    "facetId": "f-1", "facetName": "Issues", "facetVersion": 2,
                    "state": "complete", "summary": "Login fails", "summaryTruncated": true, "processedAt": TS
                }, {
                    "traceId": "t-2", "traceName": "chat", "environment": "production", "timestamp": TS,
                    "facetId": "f-1", "facetName": "Issues", "facetVersion": 2,
                    "state": "insufficient_input", "summary": null, "summaryTruncated": false, "processedAt": TS
                }],
                "meta": {"cursor": "next", "comment": "more rows may follow"}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .unstable_topics
        .list_summaries(&ListTopicSummariesParams {
            truncate_summaries: Some(true),
            order_by: Some(TopicResultsOrderBy::TimestampAsc),
            cursor: Some("c0".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].state, TopicSummaryState::Complete);
    assert_eq!(page.data[1].state, TopicSummaryState::InsufficientInput);
    assert_eq!(page.meta.cursor.as_deref(), Some("next"));
    assert_eq!(page.meta.comment.as_deref(), Some("more rows may follow"));

    let req = single_request(&server).await;
    assert_eq!(query_value(&req, "truncateSummaries"), "true");
    assert_eq!(query_value(&req, "orderBy"), "timestamp.asc");
    assert_eq!(query_value(&req, "cursor"), "c0");
}

#[tokio::test]
async fn list_assignments_last_page() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/unstable/topic-assignments"))
        .respond_with(json(
            200,
            json!({
                "window": window(),
                "data": [{
                    "traceId": "t-1", "timestamp": TS, "facetId": "f-1", "facetName": "Issues",
                    "facetVersion": 2, "ruleId": null, "status": "assigned",
                    "topic": {"id": "topic-1", "versionId": "tv-1", "name": "Login failures"},
                    "runId": null, "assignedAt": TS
                }, {
                    "traceId": "t-2", "timestamp": TS, "facetId": "f-1", "facetName": "Issues",
                    "facetVersion": 2, "status": "outlier", "assignedAt": TS
                }],
                "meta": {"cursor": null}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .unstable_topics
        .list_assignments(&ListTopicAssignmentsParams {
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.data[0].status, TopicAssignmentStatus::Assigned);
    assert_eq!(page.data[0].topic.as_ref().unwrap().name, "Login failures");
    assert_eq!(page.data[1].status, TopicAssignmentStatus::Outlier);
    assert!(page.data[1].topic.is_none());
    assert!(page.meta.cursor.is_none());
}
