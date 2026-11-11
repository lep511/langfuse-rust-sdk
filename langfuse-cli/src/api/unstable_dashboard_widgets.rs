//! `langfuse api unstable-dashboard-widgets` (feature `unstable`).

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, expand_objects, json_object, json_value};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "unstable-dashboard-widgets", "list", "unstable_dashboardWidgets_list",
        HttpMethod::Get, "/api/public/unstable/dashboard-widgets",
    )
    .columns(&["id", "name", "view", "chartType", "updatedAt"]);
}

json_op! {
    Create {}
    meta = OperationMeta::new(
        "unstable-dashboard-widgets", "create", "unstable_dashboardWidgets_create",
        HttpMethod::Post, "/api/public/unstable/dashboard-widgets",
    );
}

path_op! {
    Get {
        /// The widget ID
        widget_id => "widgetId",
    }
    meta = OperationMeta::new(
        "unstable-dashboard-widgets", "get", "unstable_dashboardWidgets_get",
        HttpMethod::Get, "/api/public/unstable/dashboard-widgets/{widgetId}",
    );
}

#[derive(Debug, Args)]
pub struct Update {
    /// The widget ID
    widget_id: String,
    #[command(flatten)]
    fields: UpdateFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpdateFields {
    /// New name
    #[arg(long)]
    name: Option<String>,
    /// New description
    #[arg(long)]
    description: Option<String>,
    /// Data view of the widget
    #[arg(long, value_parser = ["observations", "scores-numeric", "scores-boolean", "scores-categorical"])]
    view: Option<String>,
    /// Dimension as a JSON object (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    dimensions: Vec<Value>,
    /// Metric as a JSON object (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    metrics: Vec<Value>,
    /// Filter as a JSON object (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    filters: Vec<Value>,
    /// Chart type (without --chart-config the config resets to the type's defaults)
    #[arg(
        long,
        value_parser = [
            "LINE_TIME_SERIES", "AREA_TIME_SERIES", "BAR_TIME_SERIES", "HORIZONTAL_BAR",
            "VERTICAL_BAR", "PIE", "NUMBER", "HISTOGRAM", "PIVOT_TABLE",
        ]
    )]
    chart_type: Option<String>,
    /// Chart config as a JSON object
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    chart_config: Option<Value>,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "unstable-dashboard-widgets",
        "update",
        "unstable_dashboardWidgets_update",
        HttpMethod::Patch,
        "/api/public/unstable/dashboard-widgets/{widgetId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("widgetId", &self.widget_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        Ok(request.body(
            ObjectBody::new()
                .opt("name", f.name.clone())
                .opt("description", f.description.clone())
                .opt("view", f.view.clone())
                .items("dimensions", expand_objects("dimensions", &f.dimensions)?)
                .items("metrics", expand_objects("metrics", &f.metrics)?)
                .items("filters", expand_objects("filters", &f.filters)?)
                .opt("chartType", f.chart_type.clone())
                .opt("chartConfig", f.chart_config.clone())
                .build(),
        ))
    }
}

path_op! {
    Delete {
        /// The widget ID
        widget_id => "widgetId",
    }
    meta = OperationMeta::new(
        "unstable-dashboard-widgets", "delete", "unstable_dashboardWidgets_delete",
        HttpMethod::Delete, "/api/public/unstable/dashboard-widgets/{widgetId}",
    );
}

resource! {
    pub enum Command {
        /// List dashboard widgets, most recently updated first
        List(List),
        /// Create a dashboard widget (place it with `unstable-dashboards add-placement`)
        #[command(after_help = "Body fields: name, view, dimensions, metrics, filters, chartType \
            (required), description, chartConfig.")]
        Create(Create),
        /// Get a dashboard widget
        Get(Get),
        /// Update a dashboard widget (at least one field)
        Update(Update),
        /// Delete a dashboard widget (409 while it is placed on a dashboard)
        Delete(Delete),
    }
}
