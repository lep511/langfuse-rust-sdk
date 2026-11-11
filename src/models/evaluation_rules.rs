//! Evaluation rule request and response types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};

use super::evaluation_common::{
    Creator, EvaluationRuleReadFilter, PromptVariableMapping, PromptVariableMappingInput,
};

/// A live evaluation rule: which observations are scored, by which evaluators,
/// how often, and how prompt variables are populated.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRule {
    pub id: String,
    pub name: String,
    pub created_by: Option<Creator>,
    pub enabled: bool,
    /// Fraction of matching observations evaluated, between 0 and 1.
    pub sampling: f64,
    pub filter: Vec<EvaluationRuleReadFilter>,
    pub evaluator_assignments: Vec<EvaluatorAssignment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatorAssignment {
    pub evaluator_id: String,
    pub variable_mapping: Option<Vec<PromptVariableMapping>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationRuleEvaluatorAssignmentInput {
    /// The rule always uses the latest version of the evaluator.
    pub evaluator_id: String,
    /// One mapping per evaluator variable (LLM-as-a-judge evaluators only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_mapping: Option<Vec<PromptVariableMappingInput>>,
}

/// Body of `POST /api/public/v2/evaluation-rules`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEvaluationRuleRequest {
    pub name: String,
    /// An enabled rule needs at least one evaluator assignment.
    pub enabled: bool,
    /// Omit to evaluate every matching observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<f64>,
    /// Omit to match every incoming observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<EvaluationRuleFilter>>,
    pub evaluator_assignments: Vec<EvaluationRuleEvaluatorAssignmentInput>,
}

/// Body of `PATCH /api/public/v2/evaluation-rules/{id}`. Set at least one field.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvaluationRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<EvaluationRuleFilter>>,
    /// Replaces the whole assignment list; an empty list disables the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_assignments: Option<Vec<EvaluationRuleEvaluatorAssignmentInput>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedEvaluationRule {
    pub id: String,
}

/// One filter condition of an evaluation rule (spec `EvaluationRuleFilter`,
/// discriminated by `type`). All conditions must match for the rule to run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum EvaluationRuleFilter {
    #[serde(rename = "datetime")]
    DateTime {
        column: String,
        operator: NumberFilterOperator,
        value: DateTime<Utc>,
    },
    #[serde(rename = "string")]
    String {
        column: String,
        operator: StringFilterOperator,
        value: String,
    },
    #[serde(rename = "number")]
    Number {
        column: String,
        operator: NumberFilterOperator,
        value: f64,
    },
    #[serde(rename = "stringOptions")]
    StringOptions {
        column: String,
        operator: OptionsFilterOperator,
        value: Vec<String>,
    },
    #[serde(rename = "categoryOptions")]
    CategoryOptions {
        column: String,
        key: String,
        operator: OptionsFilterOperator,
        value: Vec<String>,
    },
    #[serde(rename = "arrayOptions")]
    ArrayOptions {
        column: String,
        operator: ArrayOptionsFilterOperator,
        value: Vec<String>,
    },
    /// Use `is set` / `is not set` with an empty `value` to test key presence.
    #[serde(rename = "stringObject")]
    StringObject {
        column: String,
        key: String,
        operator: StringObjectFilterOperator,
        value: String,
    },
    #[serde(rename = "numberObject")]
    NumberObject {
        column: String,
        key: String,
        operator: NumberFilterOperator,
        value: f64,
    },
    #[serde(rename = "boolean")]
    Boolean {
        column: String,
        operator: BooleanFilterOperator,
        value: bool,
    },
    #[serde(rename = "null")]
    Null {
        column: String,
        operator: NullFilterOperator,
        value: EmptyValue,
    },
}

/// The empty-string placeholder value of `null` filters. The API no longer
/// requires it (a missing `value` defaults to `""`); the SDK keeps sending
/// it, as servers older than that change expect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyValue;

impl Serialize for EmptyValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StringFilterOperator {
    #[serde(rename = "=")]
    Equals,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "does not contain")]
    DoesNotContain,
    #[serde(rename = "starts with")]
    StartsWith,
    #[serde(rename = "ends with")]
    EndsWith,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StringObjectFilterOperator {
    #[serde(rename = "=")]
    Equals,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "does not contain")]
    DoesNotContain,
    #[serde(rename = "starts with")]
    StartsWith,
    #[serde(rename = "ends with")]
    EndsWith,
    #[serde(rename = "is set")]
    IsSet,
    #[serde(rename = "is not set")]
    IsNotSet,
}

/// Comparison operator for `number`, `numberObject` and `datetime` filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NumberFilterOperator {
    #[serde(rename = "=")]
    Equals,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "<=")]
    LessThanOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OptionsFilterOperator {
    #[serde(rename = "any of")]
    AnyOf,
    #[serde(rename = "none of")]
    NoneOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ArrayOptionsFilterOperator {
    #[serde(rename = "any of")]
    AnyOf,
    #[serde(rename = "none of")]
    NoneOf,
    #[serde(rename = "all of")]
    AllOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BooleanFilterOperator {
    #[serde(rename = "=")]
    Equals,
    #[serde(rename = "<>")]
    NotEquals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NullFilterOperator {
    #[serde(rename = "is null")]
    IsNull,
    #[serde(rename = "is not null")]
    IsNotNull,
}
