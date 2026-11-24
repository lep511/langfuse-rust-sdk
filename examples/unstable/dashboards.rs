//! Full CRUD lifecycle of an unstable dashboard: create a widget,
//! create a dashboard, add a placement, update and resize it, then
//! clean up.
//!
//! **Writes data**, then cleans up: one widget and one dashboard (with
//! placements), all deleted at the end.
//!
//! **Needs:** the `unstable` feature.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example unstable_dashboards --features unstable
//! ```

use langfuse_rust_sdk::models::unstable_dashboard_widgets::{
    CreateDashboardWidgetRequest, DashboardWidgetChartType, DashboardWidgetMetric,
    DashboardWidgetMetricAggregation, DashboardWidgetView,
};
use langfuse_rust_sdk::models::unstable_dashboards::{
    CreateDashboardPlacementRequest, CreateDashboardRequest, DashboardPlacement,
    UpdateDashboardPlacementRequest, UpdateDashboardRequest,
};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

type BoxError = Box<dyn std::error::Error>;

fn placement_pos(p: &DashboardPlacement) -> (i64, i64, i64, i64) {
    match p {
        DashboardPlacement::Widget {
            x,
            y,
            width,
            height,
            ..
        }
        | DashboardPlacement::Preset {
            x,
            y,
            width,
            height,
            ..
        } => (*x, *y, *width, *height),
    }
}

fn placement_id(p: &DashboardPlacement) -> &str {
    match p {
        DashboardPlacement::Widget { id, .. } | DashboardPlacement::Preset { id, .. } => id,
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // Create a widget first (dashboards place existing widgets).
    println!("Creating a widget for the dashboard...");
    let widget = client
        .unstable_dashboard_widgets
        .create(&CreateDashboardWidgetRequest {
            name: "Rust SDK dashboard example widget".into(),
            description: None,
            view: DashboardWidgetView::Observations,
            dimensions: vec![],
            metrics: vec![DashboardWidgetMetric {
                measure: "count".into(),
                agg: DashboardWidgetMetricAggregation::Count,
            }],
            filters: vec![],
            chart_type: DashboardWidgetChartType::LineTimeSeries,
            chart_config: None,
        })
        .await?;
    println!("  Widget: {} (id: {})", widget.name, widget.id);

    // --- CREATE dashboard ---
    println!("\nCreating a dashboard...");
    let dashboard = client
        .unstable_dashboards
        .create(&CreateDashboardRequest {
            name: "Rust SDK example dashboard".into(),
            description: Some("Created by unstable_dashboards example".into()),
            definition: None,
            filters: None,
        })
        .await?;
    println!("  Dashboard: {} (id: {})", dashboard.name, dashboard.id);
    println!("  Placements: {}", dashboard.definition.widgets.len());

    // --- GET dashboard ---
    println!("\nFetching dashboard by id...");
    let fetched = client.unstable_dashboards.get(&dashboard.id).await?;
    println!("  Name: {}", fetched.name);

    // --- LIST dashboards ---
    println!("\nListing dashboards (first page)...");
    let page = client
        .unstable_dashboards
        .list(&PageParams::default())
        .await?;
    println!("  {} dashboards on page 1", page.data.len());

    // --- UPDATE dashboard ---
    println!("\nUpdating dashboard description...");
    let updated = client
        .unstable_dashboards
        .update(
            &dashboard.id,
            &UpdateDashboardRequest {
                description: Some("Updated by the Rust SDK".into()),
                ..Default::default()
            },
        )
        .await?;
    println!("  Description: {}", updated.description);

    // --- ADD PLACEMENT ---
    println!("\nAdding a widget placement...");
    let placement = client
        .unstable_dashboards
        .add_placement(
            &dashboard.id,
            &CreateDashboardPlacementRequest::Widget {
                id: None,
                widget_id: widget.id.clone(),
                x: Some(0),
                y: Some(0),
                width: Some(12),
                height: Some(4),
            },
        )
        .await?;
    let (x, y, w, h) = placement_pos(&placement);
    let pid = placement_id(&placement).to_string();
    println!("  Placed at x={x} y={y} ({w}x{h}), id={pid}");

    // --- UPDATE PLACEMENT ---
    println!("\nResizing the placement...");
    let resized = client
        .unstable_dashboards
        .update_placement(
            &dashboard.id,
            &pid,
            &UpdateDashboardPlacementRequest {
                height: Some(6),
                ..Default::default()
            },
        )
        .await?;
    let (_, _, _, h) = placement_pos(&resized);
    println!("  New height: {h}");

    // Verify the dashboard now has the placement.
    let final_dash = client.unstable_dashboards.get(&dashboard.id).await?;
    println!(
        "\nDashboard now has {} placement(s).",
        final_dash.definition.widgets.len()
    );

    // --- DELETE PLACEMENT ---
    println!("\nDeleting the placement...");
    let del_p = client
        .unstable_dashboards
        .delete_placement(&dashboard.id, &pid)
        .await?;
    println!("  {}", del_p.message);

    // --- DELETE dashboard ---
    println!("\nDeleting the dashboard...");
    let del_d = client.unstable_dashboards.delete(&dashboard.id).await?;
    println!("  {}", del_d.message);

    // --- DELETE widget ---
    println!("Deleting the widget...");
    let del_w = client.unstable_dashboard_widgets.delete(&widget.id).await?;
    println!("  {}", del_w.message);

    println!("\nDone! Dashboard lifecycle complete.");
    Ok(())
}
