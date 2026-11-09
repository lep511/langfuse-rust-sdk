#![cfg(feature = "unstable")]
//! Serde tests for the unstable API models (feature `unstable`).

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

fn assert_enum<T>(cases: &[(T, &str)], unknown: T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for (variant, wire) in cases {
        assert_eq!(serde_json::to_value(variant).unwrap(), *wire, "{variant:?}");
        let back: T = serde_json::from_value((*wire).into()).unwrap();
        assert_eq!(&back, variant);
    }
    let parsed: T = serde_json::from_str(r#""SOME_FUTURE_VALUE""#).unwrap();
    assert_eq!(parsed, unknown);
    assert!(serde_json::to_value(&unknown).is_err());
}

fn assert_serialized<T: Serialize + std::fmt::Debug>(cases: &[(T, &str)]) {
    for (variant, wire) in cases {
        assert_eq!(serde_json::to_value(variant).unwrap(), *wire, "{variant:?}");
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Dashboard widgets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dashboard_widget_enums() {
    use langfuse_rust_sdk::models::unstable_dashboard_widgets::*;
    assert_serialized(&[
        (DashboardWidgetView::Observations, "observations"),
        (DashboardWidgetView::ScoresNumeric, "scores-numeric"),
        (DashboardWidgetView::ScoresBoolean, "scores-boolean"),
        (DashboardWidgetView::ScoresCategorical, "scores-categorical"),
    ]);
    assert_enum(
        &[
            (DashboardWidgetViewWithLegacy::Observations, "observations"),
            (
                DashboardWidgetViewWithLegacy::ScoresNumeric,
                "scores-numeric",
            ),
            (
                DashboardWidgetViewWithLegacy::ScoresBoolean,
                "scores-boolean",
            ),
            (
                DashboardWidgetViewWithLegacy::ScoresCategorical,
                "scores-categorical",
            ),
            (DashboardWidgetViewWithLegacy::Traces, "traces"),
        ],
        DashboardWidgetViewWithLegacy::Unknown,
    );
    assert_enum(
        &[
            (DashboardWidgetChartType::LineTimeSeries, "LINE_TIME_SERIES"),
            (DashboardWidgetChartType::AreaTimeSeries, "AREA_TIME_SERIES"),
            (DashboardWidgetChartType::BarTimeSeries, "BAR_TIME_SERIES"),
            (DashboardWidgetChartType::HorizontalBar, "HORIZONTAL_BAR"),
            (DashboardWidgetChartType::VerticalBar, "VERTICAL_BAR"),
            (DashboardWidgetChartType::Pie, "PIE"),
            (DashboardWidgetChartType::Number, "NUMBER"),
            (DashboardWidgetChartType::Histogram, "HISTOGRAM"),
            (DashboardWidgetChartType::PivotTable, "PIVOT_TABLE"),
        ],
        DashboardWidgetChartType::Unknown,
    );
    assert_enum(
        &[
            (DashboardWidgetMetricAggregation::Sum, "sum"),
            (DashboardWidgetMetricAggregation::Avg, "avg"),
            (DashboardWidgetMetricAggregation::Count, "count"),
            (DashboardWidgetMetricAggregation::Max, "max"),
            (DashboardWidgetMetricAggregation::Min, "min"),
            (DashboardWidgetMetricAggregation::P50, "p50"),
            (DashboardWidgetMetricAggregation::P75, "p75"),
            (DashboardWidgetMetricAggregation::P90, "p90"),
            (DashboardWidgetMetricAggregation::P95, "p95"),
            (DashboardWidgetMetricAggregation::P99, "p99"),
            (DashboardWidgetMetricAggregation::Histogram, "histogram"),
            (DashboardWidgetMetricAggregation::Uniq, "uniq"),
        ],
        DashboardWidgetMetricAggregation::Unknown,
    );
    assert_enum(
        &[
            (DashboardWidgetSortOrder::Asc, "ASC"),
            (DashboardWidgetSortOrder::Desc, "DESC"),
        ],
        DashboardWidgetSortOrder::Unknown,
    );
}

#[test]
fn chart_config_input_type_is_optional() {
    use langfuse_rust_sdk::models::unstable_dashboard_widgets::*;
    let input = DashboardWidgetChartConfigInput {
        r#type: Some(DashboardWidgetChartType::Histogram),
        bins: Some(20),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&input).unwrap(),
        json!({"type": "HISTOGRAM", "bins": 20})
    );
    assert_eq!(
        serde_json::to_value(DashboardWidgetChartConfigInput::default()).unwrap(),
        json!({})
    );
}

#[test]
fn widget_filter_roundtrip() {
    use langfuse_rust_sdk::models::unstable_common::DashboardWidgetFilter;
    let wire = json!({"column": "metadata", "operator": "=", "type": "stringObject", "value": "x", "key": "tier"});
    let filter: DashboardWidgetFilter = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(filter.key.as_deref(), Some("tier"));
    assert_eq!(serde_json::to_value(&filter).unwrap(), wire);
}

// ═══════════════════════════════════════════════════════════════════════
//  Dashboards
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dashboard_placement_both_variants_roundtrip() {
    use langfuse_rust_sdk::models::unstable_dashboards::DashboardPlacement;
    let widget = json!({"type": "widget", "id": "a", "widgetId": "w", "x": 0, "y": 1, "width": 6, "height": 4});
    let preset = json!({"type": "preset", "id": "b", "presetId": "p", "x": 6, "y": 1, "width": 6, "height": 4});
    for wire in [widget, preset] {
        let placement: DashboardPlacement = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&placement).unwrap(), wire);
    }
    let unknown = json!({"type": "image", "id": "c", "x": 0, "y": 0, "width": 1, "height": 1});
    assert!(serde_json::from_value::<DashboardPlacement>(unknown).is_err());
}

#[test]
fn create_placement_request_helpers() {
    use langfuse_rust_sdk::models::unstable_dashboards::CreateDashboardPlacementRequest;
    assert_eq!(
        serde_json::to_value(CreateDashboardPlacementRequest::preset("latency")).unwrap(),
        json!({"type": "preset", "presetId": "latency"})
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Topics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn topic_enums() {
    use langfuse_rust_sdk::models::unstable_topics::*;
    assert_serialized(&[
        (TopicsOrderBy::TraceCountDesc, "traceCount.desc"),
        (TopicsOrderBy::TraceCountAsc, "traceCount.asc"),
        (TopicsOrderBy::NameAsc, "name.asc"),
        (TopicsOrderBy::NameDesc, "name.desc"),
    ]);
    assert_serialized(&[
        (TopicResultsOrderBy::TimestampDesc, "timestamp.desc"),
        (TopicResultsOrderBy::TimestampAsc, "timestamp.asc"),
    ]);
    assert_enum(
        &[
            (TopicSummaryState::Complete, "complete"),
            (TopicSummaryState::NotApplicable, "not_applicable"),
            (TopicSummaryState::InsufficientInput, "insufficient_input"),
        ],
        TopicSummaryState::Unknown,
    );
    assert_enum(
        &[
            (TopicAssignmentStatus::Assigned, "assigned"),
            (TopicAssignmentStatus::Outlier, "outlier"),
        ],
        TopicAssignmentStatus::Unknown,
    );
}
