//! `langfuse api models`.

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, date_time, expand_objects, json_value};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

json_op! {
    Create {}
    meta = OperationMeta::new(
        "models", "create", "models_create",
        HttpMethod::Post, "/api/public/models",
    );
}

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "models", "list", "models_list",
        HttpMethod::Get, "/api/public/models",
    )
    .columns(&["id", "modelName", "matchPattern", "unit", "isLangfuseManaged", "createdAt"]);
}

path_op! {
    Get {
        /// The model ID
        id => "id",
    }
    meta = OperationMeta::new(
        "models", "get", "models_get",
        HttpMethod::Get, "/api/public/models/{id}",
    );
}

#[derive(Debug, Args)]
pub struct Upsert {
    /// The model ID
    id: String,
    #[command(flatten)]
    fields: ModelFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct ModelFields {
    /// Name of the model definition
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    model_name: Option<String>,
    /// Regex matched against generation.model
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    match_pattern: Option<String>,
    /// Apply only to generations newer than this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    start_date: Option<String>,
    /// Unit of usage
    #[arg(
        long,
        value_parser = ["CHARACTERS", "TOKENS", "MILLISECONDS", "SECONDS", "IMAGES", "REQUESTS"],
        required_unless_present_any = ["body_json", "body_file"]
    )]
    unit: Option<String>,
    /// Deprecated (use --pricing-tiers): USD per input unit
    #[arg(long, value_name = "USD")]
    input_price: Option<f64>,
    /// Deprecated (use --pricing-tiers): USD per output unit
    #[arg(long, value_name = "USD")]
    output_price: Option<f64>,
    /// Deprecated (use --pricing-tiers): USD per total unit
    #[arg(long, value_name = "USD")]
    total_price: Option<f64>,
    /// Pricing tier as a JSON object (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    pricing_tiers: Vec<Value>,
    /// Legacy tokenizer identifier
    #[arg(long, value_parser = ["openai", "claude"])]
    tokenizer_id: Option<String>,
    /// Legacy tokenizer configuration (any JSON)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    tokenizer_config: Option<Value>,
}

impl Operation for Upsert {
    const META: OperationMeta = OperationMeta::new(
        "models",
        "upsert",
        "models_upsert",
        HttpMethod::Put,
        "/api/public/models/{id}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("id", &self.id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        Ok(request.body(
            ObjectBody::new()
                .opt("modelName", f.model_name.clone())
                .opt("matchPattern", f.match_pattern.clone())
                .opt("startDate", f.start_date.clone())
                .opt("unit", f.unit.clone())
                .opt("inputPrice", f.input_price)
                .opt("outputPrice", f.output_price)
                .opt("totalPrice", f.total_price)
                .items(
                    "pricingTiers",
                    expand_objects("pricing-tiers", &f.pricing_tiers)?,
                )
                .opt("tokenizerId", f.tokenizer_id.clone())
                .opt("tokenizerConfig", f.tokenizer_config.clone())
                .build(),
        ))
    }
}

path_op! {
    Delete {
        /// The model ID
        id => "id",
    }
    meta = OperationMeta::new(
        "models", "delete", "models_delete",
        HttpMethod::Delete, "/api/public/models/{id}",
    )
    .empty();
}

resource! {
    pub enum Command {
        /// Create a model definition
        #[command(after_help = "Body fields: modelName, matchPattern, unit (required), startDate, \
            pricingTiers, inputPrice, outputPrice, totalPrice, tokenizerId, tokenizerConfig.\n\n\
            Field flags are available on `langfuse api models upsert <id>`.")]
        Create(Create),
        /// Get all models
        List(List),
        /// Get a model
        Get(Get),
        /// Create or replace a project-owned model by ID (built-in models cannot change)
        Upsert(Upsert),
        /// Delete a project-owned model (Langfuse-managed models cannot be deleted)
        Delete(Delete),
    }
}
