//! Dashboard widgets (unstable API, feature `unstable`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::unstable_common::DashboardWidgetFilter;

/// Data view of a new or updated widget. The legacy `traces` view cannot be set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DashboardWidgetView {
    Observations,
    ScoresNumeric,
    ScoresBoolean,
    ScoresCategorical,
}

wire_enum!(
    /// Data view of a stored widget; may be the legacy `traces` view.
    DashboardWidgetViewWithLegacy, "kebab-case", {
    Observations,
    ScoresNumeric,
    ScoresBoolean,
    ScoresCategorical,
    Traces,
});

wire_enum!(DashboardWidgetChartType, "SCREAMING_SNAKE_CASE", {
    LineTimeSeries,
    AreaTimeSeries,
    BarTimeSeries,
    HorizontalBar,
    VerticalBar,
    Pie,
    Number,
    Histogram,
    PivotTable,
});

wire_enum!(DashboardWidgetMetricAggregation, "lowercase", {
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
    Uniq,
});

wire_enum!(DashboardWidgetSortOrder, "UPPERCASE", {
    Asc,
    Desc,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetDimension {
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetMetric {
    pub measure: String,
    pub agg: DashboardWidgetMetricAggregation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetDefaultSort {
    pub column: String,
    pub order: DashboardWidgetSortOrder,
}

/// Chart configuration of a stored widget. `row_limit` and `show_value_labels`
/// keep their snake_case wire names.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetChartConfig {
    /// Matches the widget's `chart_type`.
    pub r#type: DashboardWidgetChartType,
    /// Total-value charts and pivot tables.
    #[serde(rename = "row_limit")]
    pub row_limit: Option<i64>,
    #[serde(rename = "show_value_labels")]
    pub show_value_labels: Option<bool>,
    /// Histograms.
    pub bins: Option<i64>,
    /// Pivot tables.
    pub default_sort: Option<DashboardWidgetDefaultSort>,
}

/// Chart configuration of a request. `type` defaults to the widget's chart type.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetChartConfigInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<DashboardWidgetChartType>,
    #[serde(rename = "row_limit", skip_serializing_if = "Option::is_none")]
    pub row_limit: Option<i64>,
    #[serde(rename = "show_value_labels", skip_serializing_if = "Option::is_none")]
    pub show_value_labels: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort: Option<DashboardWidgetDefaultSort>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidget {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub name: String,
    pub description: String,
    pub view: DashboardWidgetViewWithLegacy,
    pub dimensions: Vec<DashboardWidgetDimension>,
    pub metrics: Vec<DashboardWidgetMetric>,
    pub filters: Vec<DashboardWidgetFilter>,
    pub chart_type: DashboardWidgetChartType,
    pub chart_config: DashboardWidgetChartConfig,
}

/// Body of `POST /api/public/unstable/dashboard-widgets`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDashboardWidgetRequest {
    pub name: String,
    /// Server default `""`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub view: DashboardWidgetView,
    pub dimensions: Vec<DashboardWidgetDimension>,
    pub metrics: Vec<DashboardWidgetMetric>,
    pub filters: Vec<DashboardWidgetFilter>,
    pub chart_type: DashboardWidgetChartType,
    /// Defaults to the plain config of `chart_type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<DashboardWidgetChartConfigInput>,
}

/// Body of `PATCH /api/public/unstable/dashboard-widgets/{widgetId}`. Set at least
/// one field. Changing `chart_type` without `chart_config` resets the config.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDashboardWidgetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<DashboardWidgetView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Vec<DashboardWidgetDimension>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<DashboardWidgetMetric>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<DashboardWidgetFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_type: Option<DashboardWidgetChartType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<DashboardWidgetChartConfigInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDashboardWidgetResponse {
    pub message: String,
}
