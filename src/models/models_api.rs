use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::enums::ModelUsageUnit;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub model_name: String,
    pub match_pattern: String,
    pub start_date: Option<NaiveDate>,
    pub unit: Option<ModelUsageUnit>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub total_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelRequest {
    pub model_name: String,
    pub match_pattern: String,
    pub unit: ModelUsageUnit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_price: Option<f64>,
}
