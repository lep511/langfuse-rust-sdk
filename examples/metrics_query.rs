//! Query aggregated metrics with the v2 Metrics API: generation count, token
//! usage and cost per model over the last 7 days, then a daily trend of
//! observation count and p95 latency. Read-only.
//!
//! Result rows are JSON objects keyed by dimension and by
//! `<aggregation>_<measure>`, e.g. `{"providedModelName": "...", "count_count": 41}`.
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
    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL").unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;
    Ok(LangfuseClient::new(config)?)
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

    let to = Utc::now();
    let from = to - Duration::days(7);
    let from = from.to_rfc3339_opts(SecondsFormat::Millis, true);
    let to = to.to_rfc3339_opts(SecondsFormat::Millis, true);
    println!("Window: {from} .. {to}\n");

    // 1. Generations per model: count, tokens and cost.
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
        // Order by a metric alias: `<aggregation>_<measure>`.
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

    // 2. Daily trend of all observations: count and p95 latency.
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
