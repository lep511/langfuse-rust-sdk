use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Adapter used to talk to an LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LlmAdapter {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    Openai,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "bedrock")]
    Bedrock,
    #[serde(rename = "google-vertex-ai")]
    GoogleVertexAi,
    #[serde(rename = "google-ai-studio")]
    GoogleAiStudio,
}

/// An LLM connection (secrets excluded).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConnection {
    pub id: String,
    /// Unique per project; connections are upserted on it.
    pub provider: String,
    /// Adapter name, e.g. `openai`.
    pub adapter: String,
    /// Masked secret key.
    pub display_secret_key: String,
    #[serde(rename = "baseURL")]
    pub base_url: Option<String>,
    pub custom_models: Vec<String>,
    pub with_default_models: bool,
    /// Names of the extra headers (values are not returned).
    pub extra_header_keys: Vec<String>,
    pub config: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body of the upsert endpoints (`PUT .../llm-connections`), keyed on `provider`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertLlmConnectionRequest {
    pub provider: String,
    pub adapter: LlmAdapter,
    pub secret_key: String,
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_models: Option<Vec<String>>,
    /// Server default `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_default_models: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_headers: Option<HashMap<String, String>>,
    /// Adapter-specific: required for Bedrock (`{"region": ...}`), optional for
    /// OpenAI (`{"useResponsesApi": bool}`) and Vertex AI (`{"location": ...}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

impl UpsertLlmConnectionRequest {
    /// A request with only the required fields set.
    pub fn new(
        provider: impl Into<String>,
        adapter: LlmAdapter,
        secret_key: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            adapter,
            secret_key: secret_key.into(),
            base_url: None,
            custom_models: None,
            with_default_models: None,
            extra_headers: None,
            config: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLlmConnectionResponse {
    pub message: String,
}
