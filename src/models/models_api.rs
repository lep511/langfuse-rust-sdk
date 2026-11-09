//! LLM model definition request and response types.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::ModelUsageUnit;

/// A model definition used to compute USD cost from usage (spec `Model`).
///
/// Models use either flat pricing (`input_price`, `output_price`, `prices`) or
/// tiered pricing (`pricing_tiers`). With tiered pricing, the flat fields are
/// populated from the default tier.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub model_name: String,
    pub match_pattern: String,
    pub start_date: Option<DateTime<Utc>>,
    pub unit: Option<ModelUsageUnit>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    /// Deprecated; see `prices`.
    pub total_price: Option<f64>,
    /// Legacy tokenizer identifier, kept for API compatibility.
    pub tokenizer_id: Option<String>,
    /// Legacy tokenizer configuration, kept for API compatibility.
    pub tokenizer_config: Option<serde_json::Value>,
    #[serde(default)]
    pub is_langfuse_managed: bool,
    pub created_at: DateTime<Utc>,
    /// Deprecated; prices of the default tier by usage type.
    #[serde(default)]
    pub prices: HashMap<String, ModelPrice>,
    #[serde(default)]
    pub pricing_tiers: Vec<PricingTier>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub price: f64,
}

/// A pricing tier. Tiers are evaluated in ascending `priority`; the first tier whose
/// conditions all match is used, falling back to the default tier (priority 0).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTier {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub priority: i64,
    pub conditions: Vec<PricingTierCondition>,
    /// USD price per unit, by usage type (e.g. `input`, `output`).
    pub prices: HashMap<String, f64>,
}

/// Condition of a pricing tier (spec `PricingTierCondition`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PricingTierCondition {
    Usage(PricingTierUsageCondition),
    Attribute(PricingTierAttributeCondition),
}

/// Sums the usage details whose keys match `usage_detail_pattern` (a regex)
/// and compares the sum to `value`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTierUsageCondition {
    pub usage_detail_pattern: String,
    pub operator: PricingTierOperator,
    pub value: f64,
    pub case_sensitive: bool,
}

/// Matches when the top-level attribute `key` of `source` is one of `values`.
/// Serialized with the constant `"operator": "in"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operator", rename = "in", rename_all = "camelCase")]
pub struct PricingTierAttributeCondition {
    pub source: PricingTierAttributeSource,
    pub key: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PricingTierOperator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PricingTierAttributeSource {
    ModelParameters,
    Metadata,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Legacy tokenizer identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ModelTokenizerId {
    Openai,
    Claude,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// A pricing tier to create. Exactly one tier must be the default
/// (`is_default = true`, `priority = 0`, no conditions).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTierInput {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    pub priority: i64,
    pub conditions: Vec<PricingTierConditionInput>,
    pub prices: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum PricingTierConditionInput {
    Usage(PricingTierUsageConditionInput),
    Attribute(PricingTierAttributeCondition),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTierUsageConditionInput {
    pub usage_detail_pattern: String,
    pub operator: PricingTierOperator,
    pub value: f64,
    /// Server default `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,
}

/// Body of `POST /api/public/models` and `PUT /api/public/models/{id}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelRequest {
    pub model_name: String,
    pub match_pattern: String,
    pub unit: ModelUsageUnit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_tiers: Option<Vec<PricingTierInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokenizer_id: Option<ModelTokenizerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokenizer_config: Option<serde_json::Value>,
}

impl CreateModelRequest {
    /// A request with only the required fields set.
    pub fn new(
        model_name: impl Into<String>,
        match_pattern: impl Into<String>,
        unit: ModelUsageUnit,
    ) -> Self {
        Self {
            model_name: model_name.into(),
            match_pattern: match_pattern.into(),
            unit,
            start_date: None,
            input_price: None,
            output_price: None,
            total_price: None,
            pricing_tiers: None,
            tokenizer_id: None,
            tokenizer_config: None,
        }
    }
}
