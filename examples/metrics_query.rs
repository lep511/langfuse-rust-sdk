//! Query aggregated metrics with the v2 Metrics API
//! (`GET /api/public/v2/metrics`), the API behind custom dashboards:
//! generation count, token usage and cost per model over the last 7 days,
//! then a daily trend of observation count and p95 latency.
//!
//! **Read-only:** writes no data. Each run makes two requests against the
//! Metrics API rate limit, which is low on Langfuse Cloud (100 requests per
//! day on the Hobby plan).
//!
//! A query names a view (`observations`, `scores-numeric`,
//! `scores-categorical` or `scores-boolean`), the metrics (a measure and an
//! aggregation), optional dimensions to group by, filters, and a time range.
//! Result rows are JSON objects keyed by dimension and by
//! `<aggregation>_<measure>`, e.g.
//! `{"providedModelName": "...", "count_count": 41}`; the bucket of a time
//! series is in `time_dimension`.
//!
//! **Docs:** <https://langfuse.com/docs/metrics/features/metrics-api>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example metrics_query
//! ```

use chrono::{Duration, SecondsFormat, Utc};
use langfuse_rust_sdk::models::metrics::{
    MetricsAggregation, MetricsDimension, MetricsFilter, MetricsGranularity, MetricsMetric,
    MetricsOrderBy, MetricsQuery, MetricsQueryConfig, MetricsSortDirection, MetricsTimeDimension,
    MetricsView,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

fn metric(measure: &str, aggregation: MetricsAggregation) -> MetricsMetric {
    MetricsMetric {
        measure: measure.into(),
        aggregation,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    // The time range is required, as ISO 8601 timestamps.
    let to = Utc::now();
    let from = to - Duration::days(7);
    let from = from.to_rfc3339_opts(SecondsFormat::Millis, true);
    let to = to.to_rfc3339_opts(SecondsFormat::Millis, true);
    println!("Window: {from} .. {to}\n");

    // 1. Generations per model: count, tokens and cost. One row per value of
    //    the dimension; the filter keeps only observations of type GENERATION.
    let per_model = MetricsQuery {
        dimensions: Some(vec![MetricsDimension {
            field: "providedModelName".into(),
        }]),
        filters: Some(vec![MetricsFilter {
            column: "type".into(),
            operator: "any of".into(),
            value: serde_json::json!(["GENERATION"]),
            r#type: "stringOptions".into(),
            key: None,
        }]),
        // Order by a metric alias: `<aggregation>_<measure>`. `row_limit`
        // defaults to 100 and can go up to 1,000.
        order_by: Some(vec![MetricsOrderBy {
            field: "sum_totalCost".into(),
            direction: MetricsSortDirection::Desc,
        }]),
        config: Some(MetricsQueryConfig {
            bins: None,
            row_limit: Some(20),
        }),
        ..MetricsQuery::new(
            MetricsView::Observations,
            vec![
                metric("count", MetricsAggregation::Count),
                metric("totalTokens", MetricsAggregation::Sum),
                metric("totalCost", MetricsAggregation::Sum),
            ],
            from.clone(),
            to.clone(),
        )
    };
    let rows = client.metrics.query(&per_model).await?.data;
    println!("=== Generations per model ({} rows) ===", rows.len());
    for row in &rows {
        println!("  {row}");
    }

    // 2. Daily trend of all observations: count and p95 latency, which is in
    //    milliseconds. A time dimension returns one row per bucket, also for
    //    days without data.
    let daily = MetricsQuery {
        time_dimension: Some(MetricsTimeDimension {
            granularity: MetricsGranularity::Day,
        }),
        ..MetricsQuery::new(
            MetricsView::Observations,
            vec![
                metric("count", MetricsAggregation::Count),
                metric("latency", MetricsAggregation::P95),
            ],
            from,
            to,
        )
    };
    let rows = client.metrics.query(&daily).await?.data;
    println!("\n=== Observations per day ({} rows) ===", rows.len());
    for row in &rows {
        println!("  {row}");
    }

    Ok(())
}
