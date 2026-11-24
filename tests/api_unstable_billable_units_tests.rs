#![cfg(feature = "unstable")]

mod common;

use common::{json, query_pairs, test_client};
use langfuse_rust_sdk::api::unstable_billable_units::GetBillableUnitsParams;
use langfuse_rust_sdk::models::unstable_billable_units::BillableUnitsGranularity;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::Mock;

#[tokio::test]
async fn get_billable_units_sends_params_and_parses_response() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/unstable/organizations/billable-units",
        ))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "bucket": "2026-11-01T00:00:00Z",
                    "projectId": "proj-1",
                    "count": 150,
                    "traces": 100,
                    "observations": 30,
                    "scores": 15,
                    "topicFacetSummaries": 5
                }],
                "usageRecordedFrom": "2026-01-01T00:00:00Z"
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = GetBillableUnitsParams {
        from_timestamp: "2026-11-01T00:00:00Z".into(),
        to_timestamp: "2026-11-02T00:00:00Z".into(),
        granularity: Some(BillableUnitsGranularity::Day),
    };
    let resp = client.unstable_billable_units.get(&params).await.unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].count, 150);
    assert_eq!(resp.data[0].traces, 100);
    assert_eq!(resp.data[0].observations, 30);
    assert_eq!(resp.data[0].scores, 15);
    assert_eq!(resp.data[0].topic_facet_summaries, 5);
    assert!(resp.usage_recorded_from.is_some());

    let req = common::single_request(&server).await;
    let pairs = query_pairs(&req);
    assert!(pairs.contains(&("fromTimestamp".into(), "2026-11-01T00:00:00Z".into())));
    assert!(pairs.contains(&("toTimestamp".into(), "2026-11-02T00:00:00Z".into())));
    assert!(pairs.contains(&("granularity".into(), "day".into())));
}

#[tokio::test]
async fn get_billable_units_without_granularity() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/unstable/organizations/billable-units",
        ))
        .respond_with(json(
            200,
            json!({
                "data": [],
                "usageRecordedFrom": null
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let params = GetBillableUnitsParams::new("2026-11-01T00:00:00Z", "2026-11-02T00:00:00Z");
    let resp = client.unstable_billable_units.get(&params).await.unwrap();
    assert!(resp.data.is_empty());
    assert!(resp.usage_recorded_from.is_none());

    let req = common::single_request(&server).await;
    let pairs = query_pairs(&req);
    assert!(!pairs.iter().any(|(k, _)| k == "granularity"));
}
