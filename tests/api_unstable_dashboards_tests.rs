#![cfg(feature = "unstable")]

mod common;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::models::unstable_common::DashboardWidgetFilter;
use langfuse_rust_sdk::models::unstable_dashboards::*;
use langfuse_rust_sdk::pagination::PageParams;
use serde_json::{Value, json};
use wiremock::Mock;
use wiremock::matchers::{body_json, method, path};

const BASE: &str = "/api/public/unstable/dashboards";

fn dashboard_json() -> Value {
    json!({
        "id": "d-1",
        "createdAt": TS,
        "updatedAt": TS,
        "name": "Costs",
        "description": "",
        "definition": {"widgets": [
            {"type": "widget", "id": "pl-1", "widgetId": "w-1", "x": 0, "y": 0, "width": 6, "height": 6},
            {"type": "preset", "id": "pl-2", "presetId": "traces-count", "x": 6, "y": 0, "width": 6, "height": 6}
        ]},
        "filters": [{"column": "environment", "operator": "any of", "type": "stringOptions", "value": ["production"]}]
    })
}

#[tokio::test]
async fn list_dashboards_parses_both_placement_types() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(BASE))
        .respond_with(json(
            200,
            json!({"data": [dashboard_json()], "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .unstable_dashboards
        .list(&PageParams::default())
        .await
        .unwrap();
    let widgets = &page.data[0].definition.widgets;
    assert!(
        matches!(&widgets[0], DashboardPlacement::Widget { widget_id, .. } if widget_id == "w-1")
    );
    assert!(
        matches!(&widgets[1], DashboardPlacement::Preset { preset_id, x: 6, .. } if preset_id == "traces-count")
    );
}

#[tokio::test]
async fn create_dashboard() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(BASE))
        .and(body_json(json!({
            "name": "Costs",
            "definition": {"widgets": [
                {"type": "widget", "id": "pl-1", "widgetId": "w-1", "x": 0, "y": 0, "width": 6, "height": 6}
            ]},
            "filters": [{"column": "environment", "operator": "any of", "type": "stringOptions", "value": ["production"]}]
        })))
        .respond_with(json(200, dashboard_json()))
        .expect(1)
        .mount(&server)
        .await;

    client
        .unstable_dashboards
        .create(&CreateDashboardRequest {
            name: "Costs".into(),
            description: None,
            definition: Some(DashboardDefinition {
                widgets: vec![DashboardPlacement::Widget {
                    id: "pl-1".into(),
                    widget_id: "w-1".into(),
                    x: 0,
                    y: 0,
                    width: 6,
                    height: 6,
                }],
            }),
            filters: Some(vec![DashboardWidgetFilter {
                column: "environment".into(),
                operator: "any of".into(),
                r#type: "stringOptions".into(),
                value: Some(json!(["production"])),
                key: None,
            }]),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn get_update_delete_dashboard() {
    let (client, server) = test_client().await;
    Mock::given(method("GET"))
        .and(path(format!("{BASE}/d-1")))
        .respond_with(json(200, dashboard_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/d-1")))
        .and(body_json(json!({"description": "All costs"})))
        .respond_with(json(200, dashboard_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/d-1")))
        .respond_with(json(200, json!({"message": "Dashboard deleted"})))
        .expect(1)
        .mount(&server)
        .await;

    let dashboard = client.unstable_dashboards.get("d-1").await.unwrap();
    assert_eq!(dashboard.filters.len(), 1);
    client
        .unstable_dashboards
        .update(
            "d-1",
            &UpdateDashboardRequest {
                description: Some("All costs".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let deleted = client.unstable_dashboards.delete("d-1").await.unwrap();
    assert_eq!(deleted.message, "Dashboard deleted");
}

#[tokio::test]
async fn add_placement_with_server_defaults() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/d-1/placements")))
        .and(body_json(json!({"type": "widget", "widgetId": "w-2"})))
        .respond_with(json(
            200,
            json!({"type": "widget", "id": "pl-3", "widgetId": "w-2", "x": 0, "y": 6, "width": 6, "height": 6}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let placement = client
        .unstable_dashboards
        .add_placement("d-1", &CreateDashboardPlacementRequest::widget("w-2"))
        .await
        .unwrap();
    assert!(matches!(placement, DashboardPlacement::Widget { y: 6, .. }));
}

#[tokio::test]
async fn add_preset_placement_duplicate_id_is_conflict() {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/d-1/placements")))
        .and(body_json(
            json!({"type": "preset", "id": "pl-1", "presetId": "latency", "x": 6, "y": 0}),
        ))
        .respond_with(json(
            409,
            json!({"message": "Placement exists", "code": "conflict"}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let err = client
        .unstable_dashboards
        .add_placement(
            "d-1",
            &CreateDashboardPlacementRequest::Preset {
                id: Some("pl-1".into()),
                preset_id: "latency".into(),
                x: Some(6),
                y: Some(0),
                width: None,
                height: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Conflict { .. }));
}

#[tokio::test]
async fn update_and_delete_placement() {
    let (client, server) = test_client().await;
    Mock::given(method("PATCH"))
        .and(path(format!("{BASE}/d-1/placements/pl-1")))
        .and(body_json(json!({"width": 12})))
        .respond_with(json(
            200,
            json!({"type": "widget", "id": "pl-1", "widgetId": "w-1", "x": 0, "y": 0, "width": 12, "height": 6}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{BASE}/d-1/placements/pl-1")))
        .respond_with(json(200, json!({"message": "Placement removed"})))
        .expect(1)
        .mount(&server)
        .await;

    let placement = client
        .unstable_dashboards
        .update_placement(
            "d-1",
            "pl-1",
            &UpdateDashboardPlacementRequest {
                width: Some(12),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        placement,
        DashboardPlacement::Widget { width: 12, .. }
    ));
    client
        .unstable_dashboards
        .delete_placement("d-1", "pl-1")
        .await
        .unwrap();
}
