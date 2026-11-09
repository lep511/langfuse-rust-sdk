mod common;

use common::*;
use langfuse_sdk::models::metrics::*;
use serde_json::json;
use wiremock::Mock;
use wiremock::matchers::{method, path};

#[tokio::test]
async fn query_sends_json_encoded_query_param() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/metrics"))
        .respond_with(json(
            200,
            json!({"data": [{"name": "chat", "count_count": 42}, {"name": "embed", "count_count": 7}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let query = MetricsQuery {
        dimensions: Some(vec![MetricsDimension {
            field: "name".into(),
        }]),
        filters: Some(vec![MetricsFilter {
            column: "isRootObservation".into(),
            operator: "=".into(),
            value: json!(true),
            r#type: "boolean".into(),
            key: None,
        }]),
        time_dimension: Some(MetricsTimeDimension {
            granularity: MetricsGranularity::Day,
        }),
        order_by: Some(vec![MetricsOrderBy {
            field: "count_count".into(),
            direction: MetricsSortDirection::Desc,
        }]),
        config: Some(MetricsQueryConfig {
            bins: None,
            row_limit: Some(10),
        }),
        ..MetricsQuery::new(
            MetricsView::Observations,
            vec![MetricsMetric {
                measure: "count".into(),
                aggregation: MetricsAggregation::Count,
            }],
            "2025-01-01T00:00:00.000Z",
            "2025-02-01T00:00:00.000Z",
        )
    };
    let resp = client.metrics.query(&query).await.unwrap();
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0]["count_count"], 42);

    let req = single_request(&server).await;
    let sent: serde_json::Value = serde_json::from_str(&query_value(&req, "query")).unwrap();
    assert_eq!(
        sent,
        json!({
            "view": "observations",
            "dimensions": [{"field": "name"}],
            "metrics": [{"measure": "count", "aggregation": "count"}],
            "filters": [{"column": "isRootObservation", "operator": "=", "value": true, "type": "boolean"}],
            "timeDimension": {"granularity": "day"},
            "fromTimestamp": "2025-01-01T00:00:00.000Z",
            "toTimestamp": "2025-02-01T00:00:00.000Z",
            "orderBy": [{"field": "count_count", "direction": "desc"}],
            "config": {"row_limit": 10}
        })
    );
}

#[tokio::test]
async fn minimal_query_omits_optional_sections() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/metrics"))
        .respond_with(json(200, json!({"data": []})))
        .mount(&server)
        .await;

    let query = MetricsQuery::new(
        MetricsView::ScoresNumeric,
        vec![MetricsMetric {
            measure: "value".into(),
            aggregation: MetricsAggregation::P95,
        }],
        "2025-01-01T00:00:00Z",
        "2025-01-02T00:00:00Z",
    );
    client.metrics.query(&query).await.unwrap();

    let req = single_request(&server).await;
    let sent: serde_json::Value = serde_json::from_str(&query_value(&req, "query")).unwrap();
    assert_eq!(
        sent,
        json!({
            "view": "scores-numeric",
            "metrics": [{"measure": "value", "aggregation": "p95"}],
            "fromTimestamp": "2025-01-01T00:00:00Z",
            "toTimestamp": "2025-01-02T00:00:00Z"
        })
    );
}
