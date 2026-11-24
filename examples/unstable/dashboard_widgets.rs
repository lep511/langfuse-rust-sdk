//! Full CRUD lifecycle of an unstable dashboard widget: create, get,
//! list, update and delete.
//!
//! **Writes data**, then cleans up: one widget, deleted at the end.
//!
//! **Needs:** the `unstable` feature.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example unstable_dashboard_widgets --features unstable
//! ```

use langfuse_rust_sdk::models::unstable_dashboard_widgets::{
    CreateDashboardWidgetRequest, DashboardWidgetChartType, DashboardWidgetDimension,
    DashboardWidgetMetric, DashboardWidgetMetricAggregation, DashboardWidgetView,
    UpdateDashboardWidgetRequest,
};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

type BoxError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // --- CREATE ---
    println!("Creating a dashboard widget...");
    let widget = client
        .unstable_dashboard_widgets
        .create(&CreateDashboardWidgetRequest {
            name: "Rust SDK example widget".into(),
            description: Some("Created by unstable_dashboard_widgets example".into()),
            view: DashboardWidgetView::Observations,
            dimensions: vec![DashboardWidgetDimension {
                field: "name".into(),
            }],
            metrics: vec![DashboardWidgetMetric {
                measure: "count".into(),
                agg: DashboardWidgetMetricAggregation::Count,
            }],
            filters: vec![],
            chart_type: DashboardWidgetChartType::HorizontalBar,
            chart_config: None,
        })
        .await?;
    let widget_id = widget.id.clone();
    println!("  Created: {} (id: {})", widget.name, widget.id);
    println!("  View:    {:?}", widget.view);
    println!("  Chart:   {:?}", widget.chart_type);

    // --- GET ---
    println!("\nFetching the widget by id...");
    let fetched = client.unstable_dashboard_widgets.get(&widget_id).await?;
    assert_eq!(fetched.id, widget_id);
    println!("  Name:        {}", fetched.name);
    println!("  Description: {}", fetched.description);
    println!("  Dimensions:  {:?}", fetched.dimensions);
    println!("  Metrics:     {:?}", fetched.metrics);

    // --- LIST ---
    println!("\nListing dashboard widgets (first page)...");
    let page = client
        .unstable_dashboard_widgets
        .list(&PageParams::default())
        .await?;
    println!(
        "  Page 1 of {}: {} widgets",
        page.meta.total_pages,
        page.data.len()
    );
    for w in &page.data {
        println!("    - {} ({:?})", w.name, w.chart_type);
    }

    // --- UPDATE ---
    println!("\nUpdating the widget name and chart type...");
    let updated = client
        .unstable_dashboard_widgets
        .update(
            &widget_id,
            &UpdateDashboardWidgetRequest {
                name: Some("Rust SDK example widget (updated)".into()),
                chart_type: Some(DashboardWidgetChartType::VerticalBar),
                ..Default::default()
            },
        )
        .await?;
    println!("  New name:  {}", updated.name);
    println!("  New chart: {:?}", updated.chart_type);

    // --- DELETE ---
    println!("\nDeleting the widget...");
    let deleted = client.unstable_dashboard_widgets.delete(&widget_id).await?;
    println!("  {}", deleted.message);

    println!("\nDone! Widget lifecycle complete.");
    Ok(())
}
