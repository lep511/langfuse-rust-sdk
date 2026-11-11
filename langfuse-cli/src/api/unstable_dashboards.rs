//! `langfuse api unstable-dashboards` (feature `unstable`).

use clap::{Args, ValueEnum};
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, expand_objects, json_object, json_value};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "unstable-dashboards", "list", "unstable_dashboards_list",
        HttpMethod::Get, "/api/public/unstable/dashboards",
    )
    .columns(&["id", "name", "description", "updatedAt"]);
}

json_op! {
    Create {}
    meta = OperationMeta::new(
        "unstable-dashboards", "create", "unstable_dashboards_create",
        HttpMethod::Post, "/api/public/unstable/dashboards",
    );
}

path_op! {
    Get {
        /// The dashboard ID
        dashboard_id => "dashboardId",
    }
    meta = OperationMeta::new(
        "unstable-dashboards", "get", "unstable_dashboards_get",
        HttpMethod::Get, "/api/public/unstable/dashboards/{dashboardId}",
    );
}

#[derive(Debug, Args)]
pub struct Update {
    /// The dashboard ID
    dashboard_id: String,
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
    /// New definition as a JSON object
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    definition: Option<Value>,
    /// Filter as a JSON object (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    filters: Vec<Value>,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "unstable-dashboards",
        "update",
        "unstable_dashboards_update",
        HttpMethod::Patch,
        "/api/public/unstable/dashboards/{dashboardId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request =
            PreparedRequest::new(&Self::META).path_param("dashboardId", &self.dashboard_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        Ok(request.body(
            ObjectBody::new()
                .opt("name", f.name.clone())
                .opt("description", f.description.clone())
                .opt("definition", f.definition.clone())
                .items("filters", expand_objects("filters", &f.filters)?)
                .build(),
        ))
    }
}

path_op! {
    Delete {
        /// The dashboard ID
        dashboard_id => "dashboardId",
    }
    meta = OperationMeta::new(
        "unstable-dashboards", "delete", "unstable_dashboards_delete",
        HttpMethod::Delete, "/api/public/unstable/dashboards/{dashboardId}",
    );
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlacementType {
    Widget,
    Preset,
}

/// Grid position flags.
#[derive(Debug, Args)]
#[command(next_help_heading = "Request body")]
struct Position {
    /// Grid column (12-column grid) [server default: 0]
    #[arg(long, value_name = "N")]
    x: Option<i64>,
    /// Grid row [server default: below every tile]
    #[arg(long, value_name = "N")]
    y: Option<i64>,
    /// Width in grid columns [server default: 6]
    #[arg(long, value_name = "N")]
    width: Option<i64>,
    /// Height in grid rows [server default: 6]
    #[arg(long, value_name = "N")]
    height: Option<i64>,
}

impl Position {
    fn add(&self, body: ObjectBody) -> ObjectBody {
        body.opt("x", self.x)
            .opt("y", self.y)
            .opt("width", self.width)
            .opt("height", self.height)
    }
}

#[derive(Debug, Args)]
pub struct AddPlacement {
    /// The dashboard ID
    dashboard_id: String,
    #[command(flatten)]
    fields: PlacementFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct PlacementFields {
    /// What the placement shows; selects the body variant (also sent as `type`)
    #[arg(long = "type", value_name = "TYPE", value_enum, required_unless_present_any = ["body_json", "body_file"])]
    kind: Option<PlacementType>,
    /// widget: the widget to place
    #[arg(long, required_if_eq("kind", "widget"))]
    widget_id: Option<String>,
    /// preset: the preset to place
    #[arg(long, required_if_eq("kind", "preset"))]
    preset_id: Option<String>,
    /// Placement ID [default: server-generated]
    #[arg(long)]
    id: Option<String>,
    #[command(flatten)]
    position: Position,
}

impl Operation for AddPlacement {
    const META: OperationMeta = OperationMeta::new(
        "unstable-dashboards",
        "add-placement",
        "unstable_dashboards_addPlacement",
        HttpMethod::Post,
        "/api/public/unstable/dashboards/{dashboardId}/placements",
    )
    .union("type");

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request =
            PreparedRequest::new(&Self::META).path_param("dashboardId", &self.dashboard_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        let body = match f.kind {
            Some(PlacementType::Widget) if f.preset_id.is_none() => ObjectBody::new()
                .opt("type", Some("widget"))
                .opt("id", f.id.clone())
                .opt("widgetId", f.widget_id.clone()),
            Some(PlacementType::Preset) if f.widget_id.is_none() => ObjectBody::new()
                .opt("type", Some("preset"))
                .opt("id", f.id.clone())
                .opt("presetId", f.preset_id.clone()),
            Some(_) => {
                return Err(CliError::usage(
                    "--widget-id needs --type widget and --preset-id needs --type preset",
                ));
            }
            None => {
                return Err(CliError::usage(
                    "--type is required to build the body from field flags",
                ));
            }
        };
        Ok(request.body(f.position.add(body).build()))
    }
}

#[derive(Debug, Args)]
pub struct UpdatePlacement {
    /// The dashboard ID
    dashboard_id: String,
    /// The placement ID
    placement_id: String,
    #[command(flatten)]
    fields: UpdatePlacementFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
struct UpdatePlacementFields {
    #[command(flatten)]
    position: Position,
}

impl Operation for UpdatePlacement {
    const META: OperationMeta = OperationMeta::new(
        "unstable-dashboards",
        "update-placement",
        "unstable_dashboards_updatePlacement",
        HttpMethod::Patch,
        "/api/public/unstable/dashboards/{dashboardId}/placements/{placementId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META)
            .path_param("dashboardId", &self.dashboard_id)
            .path_param("placementId", &self.placement_id);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => Ok(request.body(self.fields.position.add(ObjectBody::new()).build())),
        }
    }
}

path_op! {
    DeletePlacement {
        /// The dashboard ID
        dashboard_id => "dashboardId",
        /// The placement ID
        placement_id => "placementId",
    }
    meta = OperationMeta::new(
        "unstable-dashboards", "delete-placement", "unstable_dashboards_deletePlacement",
        HttpMethod::Delete, "/api/public/unstable/dashboards/{dashboardId}/placements/{placementId}",
    );
}

resource! {
    pub enum Command {
        /// List dashboards, most recently updated first
        List(List),
        /// Create a dashboard
        #[command(after_help = "Body fields: name (required), description, definition, filters.")]
        Create(Create),
        /// Get a dashboard
        Get(Get),
        /// Update a dashboard's name, description, definition or filters
        Update(Update),
        /// Delete a dashboard
        Delete(Delete),
        /// Place a widget or preset on a dashboard grid
        ///
        /// --type selects the variant: widget needs --widget-id, preset needs --preset-id.
        /// Without a position the placement goes below every tile.
        AddPlacement(AddPlacement),
        /// Move or resize a placement (at least one of --x, --y, --width, --height)
        UpdatePlacement(UpdatePlacement),
        /// Remove a placement (the widget is kept)
        DeletePlacement(DeletePlacement),
    }
}
