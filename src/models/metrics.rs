//! Typed query for `GET /api/public/v2/metrics`. The query is sent JSON-encoded
//! in the `query` parameter.

use serde::{Deserialize, Serialize};

/// Data view to query. The legacy `traces` view is not supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricsView {
    Observations,
    ScoresNumeric,
    ScoresBoolean,
    ScoresCategorical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsAggregation {
    Sum,
    Avg,
    Count,
    Max,
    Min,
    P50,
    P75,
    P90,
    P95,
    P99,
    Histogram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsGranularity {
    Auto,
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsSortDirection {
    Asc,
    Desc,
}

/// A field to group by. High-cardinality fields (`id`, `traceId`, `userId`,
/// `sessionId`, ...) are rejected with 400.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsDimension {
    pub field: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsMetric {
    /// What to measure, e.g. `count`, `latency`, `totalCost`.
    pub measure: String,
    pub aggregation: MetricsAggregation,
}

/// A filter condition. `operator` and the shape of `value` depend on `type`
/// (`datetime`, `string`, `number`, `stringOptions`, `categoryOptions`,
/// `arrayOptions`, `stringObject`, `numberObject`, `boolean`, `null`).
#[derive(Debug, Clone, Serialize)]
pub struct MetricsFilter {
    pub column: String,
    pub operator: String,
    pub value: serde_json::Value,
    pub r#type: String,
    /// Required for `stringObject` and `numberObject` filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsTimeDimension {
    pub granularity: MetricsGranularity,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsOrderBy {
    /// A dimension field or metric alias.
    pub field: String,
    pub direction: MetricsSortDirection,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetricsQueryConfig {
    /// Histogram bins (1–100, default 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<i64>,
    /// Maximum rows (1–1000, default 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsQuery {
    pub view: MetricsView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Vec<MetricsDimension>>,
    /// At least one metric.
    pub metrics: Vec<MetricsMetric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<MetricsFilter>>,
    /// Group results by time when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_dimension: Option<MetricsTimeDimension>,
    /// ISO 8601 start of the range.
    pub from_timestamp: String,
    /// ISO 8601 end of the range (after `from_timestamp`).
    pub to_timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<MetricsOrderBy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<MetricsQueryConfig>,
}

impl MetricsQuery {
    /// A query with only the required fields set.
    pub fn new(
        view: MetricsView,
        metrics: Vec<MetricsMetric>,
        from_timestamp: impl Into<String>,
        to_timestamp: impl Into<String>,
    ) -> Self {
        Self {
            view,
            dimensions: None,
            metrics,
            filters: None,
            time_dimension: None,
            from_timestamp: from_timestamp.into(),
            to_timestamp: to_timestamp.into(),
            order_by: None,
            config: None,
        }
    }
}

/// Result rows; their shape depends on the query. Histograms return
/// `[lower, upper, height]` tuples.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsResponse {
    pub data: Vec<serde_json::Value>,
}
