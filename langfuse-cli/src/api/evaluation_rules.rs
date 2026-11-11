//! `langfuse api evaluation-rules` (alias `evaluation-rules-v2`).

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, bool_value, expand_objects, json_value, tri_state};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

/// Field flags shared by create and update.
#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct RuleFields {
    /// Human-readable rule name (not an identifier, need not be unique)
    #[arg(long)]
    name: Option<String>,
    /// Whether live execution runs (enabled rules need an evaluator assignment)
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_enabled"
    )]
    enabled: Option<bool>,
    #[arg(long, hide = true, overrides_with = "enabled")]
    no_enabled: bool,
    /// Fraction of matching observations to evaluate, 0 to 1 [server default: 1]
    #[arg(long, value_name = "FRACTION")]
    sampling: Option<f64>,
    /// Filter condition as a JSON object (repeatable, or one JSON array); no filter matches
    /// every observation
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    filter: Vec<Value>,
    /// Evaluator assignment as a JSON object {"evaluatorId": …, "variableMapping": […]}
    /// (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    evaluator_assignments: Vec<Value>,
}

impl RuleFields {
    fn body(&self) -> Result<Value> {
        Ok(ObjectBody::new()
            .opt("name", self.name.clone())
            .opt("enabled", tri_state(self.enabled, self.no_enabled))
            .opt("sampling", self.sampling)
            .items("filter", expand_objects("filter", &self.filter)?)
            .items(
                "evaluatorAssignments",
                expand_objects("evaluator-assignments", &self.evaluator_assignments)?,
            )
            .build())
    }
}

#[derive(Debug, Args)]
pub struct Create {
    #[command(flatten)]
    fields: RuleFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "evaluation-rules",
        "create",
        "evaluationRules_create",
        HttpMethod::Post,
        "/api/public/v2/evaluation-rules",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let fields = &self.fields;
        let mut missing = Vec::new();
        if fields.name.is_none() {
            missing.push("--name");
        }
        if tri_state(fields.enabled, fields.no_enabled).is_none() {
            missing.push("--enabled");
        }
        if fields.evaluator_assignments.is_empty()
            && fields.enabled != Some(false)
            && !fields.no_enabled
        {
            missing.push("--evaluator-assignments");
        }
        if !missing.is_empty() {
            return Err(crate::error::CliError::usage(format!(
                "Missing required body option(s): {} (or pass --body-json / --body-file)",
                missing.join(", ")
            )));
        }
        let mut body = fields.body()?;
        if let Value::Object(object) = &mut body {
            object
                .entry("evaluatorAssignments")
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        Ok(request.body(body))
    }
}

#[derive(Debug, Args)]
pub struct Update {
    /// The evaluation rule ID
    evaluation_rule_id: String,
    #[command(flatten)]
    fields: RuleFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "evaluation-rules",
        "update",
        "evaluationRules_update",
        HttpMethod::Patch,
        "/api/public/v2/evaluation-rules/{evaluationRuleId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META)
            .path_param("evaluationRuleId", &self.evaluation_rule_id);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => Ok(request.body(self.fields.body()?)),
        }
    }
}

cursor_list_op! {
    List {}
    meta = OperationMeta::new(
        "evaluation-rules", "list", "evaluationRules_list",
        HttpMethod::Get, "/api/public/v2/evaluation-rules",
    )
    .columns(&["id", "name", "enabled", "sampling", "createdAt"]);
}

path_op! {
    Get {
        /// The evaluation rule ID
        evaluation_rule_id => "evaluationRuleId",
    }
    meta = OperationMeta::new(
        "evaluation-rules", "get", "evaluationRules_get",
        HttpMethod::Get, "/api/public/v2/evaluation-rules/{evaluationRuleId}",
    );
}

path_op! {
    Delete {
        /// The evaluation rule ID
        evaluation_rule_id => "evaluationRuleId",
    }
    meta = OperationMeta::new(
        "evaluation-rules", "delete", "evaluationRules_delete",
        HttpMethod::Delete, "/api/public/v2/evaluation-rules/{evaluationRuleId}",
    );
}

resource! {
    pub enum Command {
        /// Create an evaluation rule: which observations to evaluate, with which evaluators
        ///
        /// Rule names are not identifiers. Enabled rules need at least one evaluator
        /// assignment; a disabled rule may be a draft with none.
        Create(Create),
        /// List evaluation rules, newest first (legacy trace and dataset rules included)
        List(List),
        /// Get one evaluation rule
        Get(Get),
        /// Update an evaluation rule; only the given fields change
        ///
        /// --evaluator-assignments replaces the whole list (an empty list disables the rule);
        /// --filter replaces the whole filter.
        Update(Update),
        /// Delete an evaluation rule (its evaluators and scores are kept)
        Delete(Delete),
    }
}
