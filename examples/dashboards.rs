//! Build a custom dashboard with the unstable Dashboards API, pause so you
//! can look at it in the Langfuse UI, then delete it.
//!
//! The example creates three widgets (observations over time, observations by
//! name, total cost) and a dashboard that places them on the grid: one tile at
//! an explicit position, one at the position the server picks, and one resized
//! after placing. It prints a link to the dashboard and waits:
//!
//! - in a terminal: until you press Enter;
//! - with `DASHBOARD_PAUSE_SECS=<n>`: for `n` seconds (for non-interactive runs);
//! - otherwise (no terminal, variable unset): not at all;
//! - Ctrl+C ends the pause early, and cleanup still runs.
//!
//! Then it deletes the dashboard (which removes its placements) and the
//! widgets. If a step fails, cleanup still removes whatever was created.
//!
//! Set `DASHBOARD_ENVIRONMENT=production` (or another name) to add a
//! dashboard-level environment filter.
//!
//! **Writes data**, then cleans up: three widgets and one dashboard, deleted
//! at the end.
//!
//! **Needs:** the `unstable` feature. The endpoints under
//! `/api/public/unstable` may change while Langfuse finalizes the dashboard
//! and widget contract.
//!
//! One run makes 14 requests, 13 of them to the unstable endpoints, which
//! count against the general API rate limit of Langfuse Cloud (30 requests
//! per minute on the Hobby plan). On HTTP 429 the SDK waits for `Retry-After`
//! and retries, so back-to-back runs can stall for a few seconds.
//!
//! **Docs:** <https://langfuse.com/docs/metrics/features/custom-dashboards>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example dashboards --features unstable
//! ```

use std::io::{IsTerminal, Write};
use std::time::Duration;

use langfuse_rust_sdk::models::unstable_common::DashboardWidgetFilter;
use langfuse_rust_sdk::models::unstable_dashboard_widgets::{
    CreateDashboardWidgetRequest, DashboardWidget, DashboardWidgetChartType,
    DashboardWidgetDimension, DashboardWidgetMetric, DashboardWidgetMetricAggregation,
    DashboardWidgetView,
};
use langfuse_rust_sdk::models::unstable_dashboards::{
    CreateDashboardPlacementRequest, CreateDashboardRequest, DashboardPlacement,
    UpdateDashboardPlacementRequest,
};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use tokio::sync::oneshot;

type BoxError = Box<dyn std::error::Error>;

const DEFAULT_BASE_URL: &str = "https://cloud.langfuse.com";

fn base_url() -> String {
    std::env::var("LANGFUSE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into())
}

fn build_client() -> Result<LangfuseClient, BoxError> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// Everything the example created, so cleanup can remove it even after a failure.
#[derive(Default)]
struct Created {
    widgets: Vec<DashboardWidget>,
    dashboard_id: Option<String>,
}

/// A widget is a saved metrics query plus a chart type: a view, dimensions
/// to group by and metrics (a measure with an aggregation), like a query of
/// the Metrics API (see the `metrics_query` example), but without a time
/// range of its own.
fn widget_request(
    name: &str,
    dimensions: &[&str],
    measure: &str,
    agg: DashboardWidgetMetricAggregation,
    chart_type: DashboardWidgetChartType,
) -> CreateDashboardWidgetRequest {
    CreateDashboardWidgetRequest {
        name: name.into(),
        description: Some("Created by the Rust SDK dashboards example".into()),
        view: DashboardWidgetView::Observations,
        dimensions: dimensions
            .iter()
            .map(|field| DashboardWidgetDimension {
                field: (*field).into(),
            })
            .collect(),
        metrics: vec![DashboardWidgetMetric {
            measure: measure.into(),
            agg,
        }],
        filters: vec![],
        chart_type,
        chart_config: None,
    }
}

/// Grid position of a placement: `(id, x, y, width, height)`.
fn tile(placement: &DashboardPlacement) -> (&str, i64, i64, i64, i64) {
    match placement {
        DashboardPlacement::Widget {
            id,
            x,
            y,
            width,
            height,
            ..
        }
        | DashboardPlacement::Preset {
            id,
            x,
            y,
            width,
            height,
            ..
        } => (id, *x, *y, *width, *height),
    }
}

/// Create the widgets and the dashboard, recording every id in `created`.
async fn build(client: &LangfuseClient, created: &mut Created) -> Result<String, BoxError> {
    use DashboardWidgetChartType as Chart;
    use DashboardWidgetMetricAggregation as Agg;

    let specs = [
        widget_request(
            "Rust SDK example: observations over time",
            &[],
            "count",
            Agg::Count,
            Chart::LineTimeSeries,
        ),
        widget_request(
            "Rust SDK example: observations by name",
            &["name"],
            "count",
            Agg::Count,
            Chart::HorizontalBar,
        ),
        widget_request(
            "Rust SDK example: total cost",
            &[],
            "totalCost",
            Agg::Sum,
            Chart::Number,
        ),
    ];
    // Widgets exist on their own and can be placed on several dashboards.
    for spec in &specs {
        let widget = client.unstable_dashboard_widgets.create(spec).await?;
        println!("Created widget '{}' ({:?})", widget.name, widget.chart_type);
        created.widgets.push(widget);
    }
    let [over_time, by_name, cost] = [0, 1, 2].map(|i| created.widgets[i].id.clone());

    // A dashboard-level filter applies to every widget placed on it.
    let filters = std::env::var("DASHBOARD_ENVIRONMENT").ok().map(|env| {
        vec![DashboardWidgetFilter {
            column: "environment".into(),
            operator: "any of".into(),
            r#type: "stringOptions".into(),
            value: Some(serde_json::json!([env])),
            key: None,
        }]
    });
    let dashboard = client
        .unstable_dashboards
        .create(&CreateDashboardRequest {
            name: "Rust SDK example dashboard".into(),
            description: Some("Created by the Rust SDK; deleted when the example ends".into()),
            definition: None,
            filters,
        })
        .await?;
    created.dashboard_id = Some(dashboard.id.clone());
    println!("Created dashboard '{}'", dashboard.name);

    // 1. Explicit position: a full-width time series on top (the grid has 12 columns).
    let top = client
        .unstable_dashboards
        .add_placement(
            &dashboard.id,
            &CreateDashboardPlacementRequest::Widget {
                id: None,
                widget_id: over_time,
                x: Some(0),
                y: Some(0),
                width: Some(12),
                height: Some(5),
            },
        )
        .await?;
    let (_, x, y, width, height) = tile(&top);
    println!("Placed the time series at x={x} y={y} ({width}x{height})");

    // 2. Server-chosen position: appended below the existing tiles as a 6x6 tile.
    let left = client
        .unstable_dashboards
        .add_placement(
            &dashboard.id,
            &CreateDashboardPlacementRequest::widget(by_name),
        )
        .await?;
    let (_, _, left_y, _, left_height) = tile(&left);
    println!("Server placed the next tile at y={left_y}");

    // 3. Next to the server-placed tile, first small, then resized to match it.
    let right = client
        .unstable_dashboards
        .add_placement(
            &dashboard.id,
            &CreateDashboardPlacementRequest::Widget {
                id: None,
                widget_id: cost,
                x: Some(6),
                y: Some(left_y),
                width: Some(6),
                height: Some(2),
            },
        )
        .await?;
    let right_id = tile(&right).0.to_string();
    client
        .unstable_dashboards
        .update_placement(
            &dashboard.id,
            &right_id,
            &UpdateDashboardPlacementRequest {
                height: Some(left_height),
                ..Default::default()
            },
        )
        .await?;

    Ok(dashboard.id)
}

/// Print the dashboard as stored by the server.
async fn show(
    client: &LangfuseClient,
    created: &Created,
    dashboard_id: &str,
) -> Result<(), BoxError> {
    let dashboard = client.unstable_dashboards.get(dashboard_id).await?;
    println!("\nLayout of '{}':", dashboard.name);
    for placement in &dashboard.definition.widgets {
        let (_, x, y, width, height) = tile(placement);
        let name = match placement {
            DashboardPlacement::Widget { widget_id, .. } => created
                .widgets
                .iter()
                .find(|w| &w.id == widget_id)
                .map_or(widget_id.as_str(), |w| w.name.as_str()),
            DashboardPlacement::Preset { preset_id, .. } => preset_id,
        };
        println!("  x={x:<2} y={y:<2} {width:>2}x{height:<2} {name}");
    }
    for filter in &dashboard.filters {
        println!(
            "  filter: {} {} {}",
            filter.column,
            filter.operator,
            filter.value.clone().unwrap_or_default()
        );
    }

    let projects = client.projects.list().await?;
    if let Some(project) = projects.data.first() {
        println!(
            "\nOpen it in Langfuse: {}/project/{}/dashboards/{}",
            base_url().trim_end_matches('/'),
            project.id,
            dashboard.id
        );
    }
    Ok(())
}

/// Wait for Enter, `DASHBOARD_PAUSE_SECS` seconds, or Ctrl+C.
async fn pause() -> Result<(), BoxError> {
    let wait = async {
        if let Ok(secs) = std::env::var("DASHBOARD_PAUSE_SECS") {
            let secs: u64 = secs
                .parse()
                .map_err(|_| "DASHBOARD_PAUSE_SECS must be a number of seconds")?;
            println!("\nDeleting in {secs} s (Ctrl+C to delete now)...");
            tokio::time::sleep(Duration::from_secs(secs)).await;
        } else if std::io::stdin().is_terminal() {
            print!("\nPress Enter to delete the dashboard and its widgets... ");
            std::io::stdout().flush()?;
            // A plain thread, so a pending read never blocks the runtime from shutting down.
            let (tx, rx) = oneshot::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                let _ = tx.send(());
            });
            let _ = rx.await;
        } else {
            println!("\nNot a terminal and DASHBOARD_PAUSE_SECS is unset: deleting right away.");
        }
        Ok::<(), BoxError>(())
    };
    tokio::select! {
        result = wait => result,
        _ = tokio::signal::ctrl_c() => {
            println!("\nInterrupted: cleaning up.");
            Ok(())
        }
    }
}

/// Delete the dashboard, then the widgets. Keeps going after a failure and
/// reports every step that failed.
async fn cleanup(client: &LangfuseClient, created: &Created) -> Result<(), BoxError> {
    let mut failures = Vec::new();
    // Deleting the dashboard removes its placements. Widgets can only be
    // deleted once no dashboard uses them (409 Conflict otherwise).
    if let Some(dashboard_id) = &created.dashboard_id {
        match client.unstable_dashboards.delete(dashboard_id).await {
            Ok(_) => println!("Deleted dashboard {dashboard_id}"),
            Err(e) => failures.push(format!("dashboard {dashboard_id}: {e}")),
        }
    }
    for widget in &created.widgets {
        match client.unstable_dashboard_widgets.delete(&widget.id).await {
            Ok(_) => println!("Deleted widget '{}'", widget.name),
            Err(e) => failures.push(format!("widget {}: {e}", widget.id)),
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    for failure in &failures {
        eprintln!("Cleanup failed for {failure}");
    }
    Err(format!(
        "{} cleanup step(s) failed; delete the leftovers in the Langfuse UI",
        failures.len()
    )
    .into())
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;

    let mut created = Created::default();
    let outcome = match build(&client, &mut created).await {
        Ok(dashboard_id) => match show(&client, &created, &dashboard_id).await {
            Ok(()) => pause().await,
            Err(e) => Err(e),
        },
        Err(e) => Err(e),
    };
    if let Err(e) = &outcome {
        eprintln!("\nError: {e}");
    }

    println!();
    let cleaned = cleanup(&client, &created).await;
    outcome.and(cleaned)
}
