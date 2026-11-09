use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasePrompt {
    pub name: String,
    pub version: i64,
    pub config: serde_json::Value,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub commit_message: Option<String>,
    pub resolution_graph: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Prompt {
    #[serde(rename_all = "camelCase")]
    Chat {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: Vec<ChatMessage>,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CreatePromptRequest {
    #[serde(rename_all = "camelCase")]
    Chat {
        name: String,
        prompt: Vec<ChatMessage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        labels: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        name: String,
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        labels: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_message: Option<String>,
    },
}
