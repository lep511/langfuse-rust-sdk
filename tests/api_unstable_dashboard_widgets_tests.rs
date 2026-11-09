#![cfg(feature = "unstable")]

mod common;

use common::*;
use langfuse_sdk::LangfuseError;
use langfuse_sdk::models::evaluation_common::PublicApiErrorCode;
use langfuse_sdk::models::unstable_common::{DashboardWidgetFilter, UnstablePublicApiError};
use langfuse_sdk::models::unstable_dashboard_widgets::*;
use langfuse_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path, query_param};

const BASE: &str = "/api/public/unstable/dashboard-widgets";

fn widget_json(view: &str) -> Value {
    json!({
        "id": "w-1",
        "createdAt": TS,
        "updatedAt": TS,
        "name": "Cost by model",
        "description": "",
        "view": view,
        "dimensions": [{"field": "providedModelName"}],
        "metrics": [{"measure": "totalCost", "agg": "sum"}],
        "filters": [{"column": "environment", "operator": "any of", "type": "stringOptions", "value": ["production"], "key": null}],
        "chartType": "PIVOT_TABLE",
        "chartConfig": {
            "type": "PIVOT_TABLE",
            "row_limit": 20,
            "show_value_labels": null,
            "bins": null,
            "defaultSort": {"column": "sum_totalCost", "order": "DESC"}
        }
    })
}

#[tokio::test]
async fn list_widgets_parses_legacy_traces_view() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .and(query_param("limit", "5"))
        .respond_with(json(
            200,
            json!({"data": [widget_json("traces")], "meta": {"page": 1, "limit": 5, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .unstable_dashboard_widgets
        .list(&PageParams {
            page: None,
            limit: Some(5),
        })
        .await
        .unwrap();
    let widget = &page.data[0];
    assert_eq!(widget.view, DashboardWidgetViewWithLegacy::Traces);
    assert_eq!(widget.chart_type, DashboardWidgetChartType::PivotTable);
    assert_eq!(widget.chart_config.row_limit, Some(20));
    assert_eq!(
        widget.chart_config.default_sort.as_ref().unwrap().order,
        DashboardWidgetSortOrder::Desc
    );
    assert_eq!(widget.metrics[0].agg, DashboardWidgetMetricAggregation::Sum);
}

#[tokio::test]
async fn create_widget_uses_snake_case_chart_config_keys() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "name": "Cost by model",
            "view": "observations",
            "dimensions": [{"field": "providedModelName"}],
            "metrics": [{"measure": "totalCost", "agg": "sum"}],
            "filters": [{"column": "environment", "operator": "any of", "type": "stringOptions", "value": ["production"]}],
            "chartType": "PIVOT_TABLE",
            "chartConfig": {"row_limit": 20, "show_value_labels": true, "defaultSort": {"column": "sum_totalCost", "order": "DESC"}}
        })))
        .respond_with(json(200, widget_json("observations")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .unstable_dashboard_widgets
        .create(&CreateDashboardWidgetRequest {
            name: "Cost by model".into(),
            description: None,
            view: DashboardWidgetView::Observations,
            dimensions: vec![DashboardWidgetDimension {
                field: "providedModelName".into(),
            }],
            metrics: vec![DashboardWidgetMetric {
                measure: "totalCost".into(),
                agg: DashboardWidgetMetricAggregation::Sum,
            }],
            filters: vec![DashboardWidgetFilter {
                column: "environment".into(),
                operator: "any of".into(),
                r#type: "stringOptions".into(),
                value: Some(json!(["production"])),
                key: None,
            }],
            chart_type: DashboardWidgetChartType::PivotTable,
            chart_config: Some(DashboardWidgetChartConfigInput {
                row_limit: Some(20),
                show_value_labels: Some(true),
                default_sort: Some(DashboardWidgetDefaultSort {
                    column: "sum_totalCost".into(),
                    order: DashboardWidgetSortOrder::Desc,
                }),
                ..Default::default()
            }),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn get_widget() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/w-1")))
        .respond_with(json(200, widget_json("scores-numeric")))
        .expect(1)
        .mount(&server)
        .await;

    let widget = client.unstable_dashboard_widgets.get("w-1").await.unwrap();
    assert_eq!(widget.view, DashboardWidgetViewWithLegacy::ScoresNumeric);
}

#[tokio::test]
async fn update_widget_sends_only_set_fields() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/w-1")))
        .and(body_json(json!({"name": "Renamed", "chartType": "NUMBER"})))
        .respond_with(json(200, widget_json("observations")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .unstable_dashboard_widgets
        .update(
            "w-1",
            &UpdateDashboardWidgetRequest {
                name: Some("Renamed".into()),
                chart_type: Some(DashboardWidgetChartType::Number),
                ..Default::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_widget() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/w-1")))
        .respond_with(json(200, json!({"message": "Widget deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let resp = client
        .unstable_dashboard_widgets
        .delete("w-1")
        .await
        .unwrap();
    assert_eq!(resp.message, "Widget deleted");
}

#[tokio::test]
async fn delete_placed_widget_is_conflict_with_error_body() {
    let (client, server) = test_client().await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/w-1")))
        .respond_with(json(
            409,
            json!({
                "message": "Widget is still placed on 2 dashboards",
                "code": "conflict",
                "details": {"field": "widgetId", "allowedValues": null, "resetAt": "soon"}
            }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .unstable_dashboard_widgets
        .delete("w-1")
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Conflict { .. }));
    let api_error = UnstablePublicApiError::from_error(&err).unwrap();
    assert_eq!(api_error.code, PublicApiErrorCode::Conflict);
    let details = api_error.details.unwrap();
    assert_eq!(details.field.as_deref(), Some("widgetId"));
    assert_eq!(details.reset_at.as_deref(), Some("soon"));
}
