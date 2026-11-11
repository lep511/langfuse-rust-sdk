//! `langfuse api evaluators` (alias `evaluators-v2`).

use clap::{Args, ValueEnum};
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, expand_objects, json_object, json_value};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvaluatorType {
    #[value(name = "llm_as_judge")]
    LlmAsJudge,
    #[value(name = "code")]
    Code,
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
    /// Evaluator type; selects the body variant (also sent as `type`)
    #[arg(long = "type", value_name = "TYPE", value_enum, required_unless_present_any = ["body_json", "body_file"])]
    kind: Option<EvaluatorType>,
    /// Human-readable name (not an identifier, need not be unique)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    name: Option<String>,
    /// Description
    #[arg(long)]
    description: Option<String>,
    /// llm_as_judge: the user prompt as a string, or chat messages as a JSON array
    #[arg(long, required_if_eq("kind", "llm_as_judge"))]
    prompt: Option<String>,
    /// llm_as_judge: structured output definition (JSON object)
    #[arg(long, value_parser = json_object, value_name = "JSON", required_if_eq("kind", "llm_as_judge"))]
    output_definition: Option<Value>,
    /// llm_as_judge: model configuration (JSON object)
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    model_config: Option<Value>,
    /// llm_as_judge: default prompt-variable mapping as JSON objects (repeatable, or one JSON array)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    variable_mapping: Vec<Value>,
    /// code: source code run for each matched observation
    #[arg(long, required_if_eq("kind", "code"))]
    source_code: Option<String>,
    /// code: runtime language
    #[arg(long, value_parser = ["PYTHON", "TYPESCRIPT"], required_if_eq("kind", "code"))]
    source_code_language: Option<String>,
}

impl CreateFields {
    fn body(&self) -> Result<Value> {
        let body = ObjectBody::new()
            .opt("name", self.name.clone())
            .opt("description", self.description.clone());
        match self.kind {
            Some(EvaluatorType::LlmAsJudge) => {
                if self.source_code.is_some() || self.source_code_language.is_some() {
                    return Err(CliError::usage(
                        "--source-code and --source-code-language need --type code",
                    ));
                }
                let prompt = self
                    .prompt
                    .as_deref()
                    .map(|prompt| match json_value(prompt) {
                        Ok(messages @ Value::Array(_)) => messages,
                        _ => Value::String(prompt.to_owned()),
                    });
                Ok(body
                    .opt("type", Some("llm_as_judge"))
                    .opt("prompt", prompt)
                    .opt("modelConfig", self.model_config.clone())
                    .items(
                        "variableMapping",
                        expand_objects("variable-mapping", &self.variable_mapping)?,
                    )
                    .opt("outputDefinition", self.output_definition.clone())
                    .build())
            }
            Some(EvaluatorType::Code) => {
                if self.prompt.is_some()
                    || self.output_definition.is_some()
                    || self.model_config.is_some()
                    || !self.variable_mapping.is_empty()
                {
                    return Err(CliError::usage(
                        "--prompt, --output-definition, --model-config and --variable-mapping need \
                         --type llm_as_judge",
                    ));
                }
                Ok(body
                    .opt("type", Some("code"))
                    .opt("sourceCode", self.source_code.clone())
                    .opt("sourceCodeLanguage", self.source_code_language.clone())
                    .build())
            }
            None => Err(CliError::usage(
                "--type is required to build the body from field flags",
            )),
        }
    }
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "evaluators",
        "create",
        "evaluators_create",
        HttpMethod::Post,
        "/api/public/v2/evaluators",
    )
    .union("type");

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => Ok(request.body(self.fields.body()?)),
        }
    }
}

cursor_list_op! {
    List {}
    meta = OperationMeta::new(
        "evaluators", "list", "evaluators_list",
        HttpMethod::Get, "/api/public/v2/evaluators",
    )
    .columns(&["id", "name", "type", "version", "createdAt"]);
}

path_op! {
    Get {
        /// The evaluator ID
        evaluator_id => "evaluatorId",
    }
    meta = OperationMeta::new(
        "evaluators", "get", "evaluators_get",
        HttpMethod::Get, "/api/public/v2/evaluators/{evaluatorId}",
    );
}

json_op! {
    Update {
        /// The evaluator ID
        evaluator_id => "evaluatorId",
    }
    meta = OperationMeta::new(
        "evaluators", "update", "evaluators_update",
        HttpMethod::Patch, "/api/public/v2/evaluators/{evaluatorId}",
    );
}

path_op! {
    Delete {
        /// The evaluator ID
        evaluator_id => "evaluatorId",
    }
    meta = OperationMeta::new(
        "evaluators", "delete", "evaluators_delete",
        HttpMethod::Delete, "/api/public/v2/evaluators/{evaluatorId}",
    );
}

cursor_list_op! {
    ListVersions {
        /// The evaluator ID
        evaluator_id => "evaluatorId",
    }
    meta = OperationMeta::new(
        "evaluators", "list-versions", "evaluators_listVersions",
        HttpMethod::Get, "/api/public/v2/evaluators/{evaluatorId}/versions",
    )
    .columns(&["version", "type", "createdAt"]);
}

resource! {
    pub enum Command {
        /// Create an evaluator: an LLM-as-a-judge prompt or code that scores observations
        ///
        /// --type selects the variant: llm_as_judge needs --prompt and --output-definition; code
        /// needs --source-code and --source-code-language.
        Create(Create),
        /// List evaluators, newest first, with their latest definition
        List(List),
        /// Get one evaluator with its latest definition and evaluation rules
        Get(Get),
        /// Update an evaluator; only the given top-level fields change
        #[command(after_help = "The body is one of: {name, description} (metadata only, no new \
            version); {type: \"llm_as_judge\", prompt, outputDefinition, modelConfig?, \
            variableMapping?, …}; {type: \"code\", sourceCode, sourceCodeLanguage, …}. The type \
            cannot change.")]
        Update(Update),
        /// Delete an evaluator and its versions (scores already produced are kept)
        Delete(Delete),
        /// List an evaluator's version history, newest first
        ListVersions(ListVersions),
    }
}
