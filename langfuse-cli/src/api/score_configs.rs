//! `langfuse api score-configs`.

use clap::Args;
use langfuse_rust_sdk::models::enums::ScoreConfigDataType;
use langfuse_rust_sdk::models::score_configs::{
    CreateConfigCategory, CreateScoreConfigRequest, UpdateScoreConfigRequest,
};
use serde_json::Value;

use crate::args::{BodyChannel, bool_value, expand_objects, json_value, required, tri_state, wire};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

fn categories(values: &[Value]) -> Result<Option<Vec<CreateConfigCategory>>> {
    let items = expand_objects("categories", values)?;
    if items.is_empty() {
        return Ok(None);
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let label = item.get("label").and_then(Value::as_str);
            let value = item.get("value").and_then(Value::as_f64);
            match (label, value) {
                (Some(label), Some(value)) => Ok(CreateConfigCategory {
                    value,
                    label: label.to_owned(),
                }),
                _ => Err(CliError::usage(format!(
                    "--categories: item {index} needs a string `label` and a numeric `value`"
                ))),
            }
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

#[derive(Debug, Args)]
pub struct Create {
    #[command(flatten)]
    fields: CreateFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct CreateFields {
    /// Name (at most 80 characters)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    name: Option<String>,
    /// Data type of the scores
    #[arg(
        long,
        value_parser = wire::<ScoreConfigDataType>(&["NUMERIC", "BOOLEAN", "CATEGORICAL", "TEXT"]),
        required_unless_present_any = ["body_json", "body_file"]
    )]
    data_type: Option<ScoreConfigDataType>,
    /// Category {"label": …, "value": …} of categorical scores (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    categories: Vec<Value>,
    /// Minimum of numeric scores [default: -∞]
    #[arg(long, value_name = "NUMBER")]
    min_value: Option<f64>,
    /// Maximum of numeric scores [default: +∞]
    #[arg(long, value_name = "NUMBER")]
    max_value: Option<f64>,
    /// Description shown in the Langfuse UI
    #[arg(long)]
    description: Option<String>,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "score-configs",
        "create",
        "scoreConfigs_create",
        HttpMethod::Post,
        "/api/public/score-configs",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        request.body_from(&CreateScoreConfigRequest {
            name: f.name.clone().unwrap_or_default(),
            data_type: required("data-type", f.data_type.clone())?,
            categories: categories(&f.categories)?,
            min_value: f.min_value,
            max_value: f.max_value,
            description: f.description.clone(),
        })
    }
}

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "score-configs", "list", "scoreConfigs_get",
        HttpMethod::Get, "/api/public/score-configs",
    )
    .columns(&["id", "name", "dataType", "isArchived", "minValue", "maxValue"]);
}

path_op! {
    Get {
        /// The score config ID
        config_id => "configId",
    }
    meta = OperationMeta::new(
        "score-configs", "get", "scoreConfigs_get-by-id",
        HttpMethod::Get, "/api/public/score-configs/{configId}",
    );
}

#[derive(Debug, Args)]
pub struct Update {
    /// The score config ID
    config_id: String,
    #[command(flatten)]
    fields: UpdateFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpdateFields {
    /// Archive (or, with =false, unarchive) the config
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_is_archived"
    )]
    is_archived: Option<bool>,
    #[arg(long, hide = true, overrides_with = "is_archived")]
    no_is_archived: bool,
    /// New name (at most 80 characters)
    #[arg(long)]
    name: Option<String>,
    /// Category {"label": …, "value": …} (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    categories: Vec<Value>,
    /// Minimum of numeric scores
    #[arg(long, value_name = "NUMBER")]
    min_value: Option<f64>,
    /// Maximum of numeric scores
    #[arg(long, value_name = "NUMBER")]
    max_value: Option<f64>,
    /// Description shown in the Langfuse UI
    #[arg(long)]
    description: Option<String>,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "score-configs",
        "update",
        "scoreConfigs_update",
        HttpMethod::Patch,
        "/api/public/score-configs/{configId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("configId", &self.config_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        request.body_from(&UpdateScoreConfigRequest {
            is_archived: tri_state(f.is_archived, f.no_is_archived),
            name: f.name.clone(),
            categories: categories(&f.categories)?,
            min_value: f.min_value,
            max_value: f.max_value,
            description: f.description.clone(),
        })
    }
}

resource! {
    pub enum Command {
        /// Create a score config, the schema of scores
        Create(Create),
        /// Get all score configs
        List(List),
        /// Get a score config
        Get(Get),
        /// Update a score config
        Update(Update),
    }
}
