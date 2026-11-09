use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::evaluation_common::{
    Creator, PromptVariableMapping, PromptVariableMappingInput, PromptVariableMappingRead,
};

wire_enum!(
    /// Runtime status of an evaluator.
    EvaluatorStatus, "lowercase", {
    Active,
    /// Paused by Langfuse until the underlying issue is resolved.
    Paused,
});

wire_enum!(CodeEvaluatorSourceCodeLanguage, "SCREAMING_SNAKE_CASE", {
    Python,
    Typescript,
});

wire_enum!(EvaluatorChatMessageRole, "lowercase", {
    System,
    User,
    Assistant,
});

/// One message of an evaluator prompt. A system message is only allowed first.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatorChatMessage {
    pub role: EvaluatorChatMessageRole,
    pub content: String,
}

/// Prompt of an evaluator request: a user prompt string, or chat messages.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EvaluatorChatPromptInput {
    Text(String),
    Messages(Vec<EvaluatorChatMessage>),
}

/// Model used by an LLM-as-a-judge evaluator. `None` selects the project's
/// default evaluation model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatorModelConfig {
    /// A `provider` from `GET /api/public/llm-connections`.
    pub provider: String,
    pub model: String,
}

/// Structured score produced by an evaluator (discriminated by `dataType`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "dataType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluatorOutputDefinition {
    #[serde(rename_all = "camelCase")]
    Numeric {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_reasoning_instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_value_instructions: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Boolean {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_reasoning_instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_value_instructions: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Categorical {
        /// At least two unique values.
        categories: Vec<String>,
        should_allow_multiple_matches: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_reasoning_instructions: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score_value_instructions: Option<String>,
    },
}

/// Definition of an evaluator version (discriminated by `type`).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvaluatorDefinition {
    #[serde(rename_all = "camelCase")]
    LlmAsJudge {
        prompt: Vec<EvaluatorChatMessage>,
        /// Variables found in the prompt; map each one in `variable_mapping`.
        variables: Vec<String>,
        variable_mapping: Option<Vec<PromptVariableMappingRead>>,
        model_config: Option<EvaluatorModelConfig>,
        output_definition: EvaluatorOutputDefinition,
    },
    #[serde(rename_all = "camelCase")]
    Code {
        source_code: String,
        source_code_language: CodeEvaluatorSourceCodeLanguage,
    },
}

/// Fields shared by both evaluator types (spec `EvaluatorBase`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatorBase {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<Creator>,
    pub status: EvaluatorStatus,
    pub paused_at: Option<DateTime<Utc>>,
    pub paused_reason: Option<String>,
    pub paused_message: Option<String>,
    pub evaluation_rule_assignments: Vec<EvaluationRuleAssignment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version_id: String,
    pub version: i64,
    pub version_created_at: DateTime<Utc>,
    pub version_created_by: Option<Creator>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRuleAssignment {
    pub evaluation_rule_id: String,
    pub variable_mapping_override: Option<Vec<PromptVariableMapping>>,
}

/// An evaluator with its latest definition (spec `Evaluator`).
#[derive(Debug, Clone, Deserialize)]
pub struct Evaluator {
    #[serde(flatten)]
    pub base: EvaluatorBase,
    #[serde(flatten)]
    pub definition: EvaluatorDefinition,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatorVersionBase {
    pub id: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<Creator>,
}

/// One stored version of an evaluator (spec `EvaluatorVersion`).
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluatorVersion {
    #[serde(flatten)]
    pub base: EvaluatorVersionBase,
    #[serde(flatten)]
    pub definition: EvaluatorDefinition,
}

/// Body of `POST /api/public/v2/evaluators`. The evaluator starts at version 1.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CreateEvaluatorRequest {
    #[serde(rename_all = "camelCase")]
    LlmAsJudge {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        prompt: EvaluatorChatPromptInput,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_config: Option<EvaluatorModelConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        variable_mapping: Option<Vec<PromptVariableMappingInput>>,
        output_definition: EvaluatorOutputDefinition,
    },
    #[serde(rename_all = "camelCase")]
    Code {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        source_code: String,
        source_code_language: CodeEvaluatorSourceCodeLanguage,
    },
}

/// Body of `PATCH /api/public/v2/evaluators/{evaluatorId}`: a metadata-only update
/// or a complete replacement of the definition (the type cannot change).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum UpdateEvaluatorRequest {
    /// Does not create a new version.
    Metadata(UpdateEvaluatorMetadataRequest),
    LlmAsJudge(UpdateLlmAsJudgeEvaluatorRequest),
    Code(UpdateCodeEvaluatorRequest),
}

/// `description`: `None` keeps it, `Some(None)` clears it, `Some(Some(_))` sets it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvaluatorMetadataRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
}

/// Serialized with `"type": "llm_as_judge"`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename = "llm_as_judge", rename_all = "camelCase")]
pub struct UpdateLlmAsJudgeEvaluatorRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    pub prompt: EvaluatorChatPromptInput,
    /// `None` selects the project's default evaluation model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config: Option<EvaluatorModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_mapping: Option<Vec<PromptVariableMappingInput>>,
    pub output_definition: EvaluatorOutputDefinition,
}

/// Serialized with `"type": "code"`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename = "code", rename_all = "camelCase")]
pub struct UpdateCodeEvaluatorRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    pub source_code: String,
    pub source_code_language: CodeEvaluatorSourceCodeLanguage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedEvaluator {
    pub id: String,
}
