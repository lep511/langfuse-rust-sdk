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
    Chat {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: Vec<ChatPromptMessage>,
    },
    Text {
        #[serde(flatten)]
        base: BasePrompt,
        prompt: String,
    },
}

/// A regular chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A placeholder in a chat prompt, filled with a list of messages at compile time.
/// Serialized as `{"type": "placeholder", "name": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename = "placeholder")]
pub struct PlaceholderMessage {
    pub name: String,
}

/// One entry of a chat prompt (spec `ChatMessageWithPlaceholders`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatPromptMessage {
    Message(ChatMessage),
    Placeholder(PlaceholderMessage),
}

impl ChatPromptMessage {
    /// A regular message with `role` and `content`.
    pub fn message(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Message(ChatMessage {
            role: role.into(),
            content: content.into(),
        })
    }

    /// A placeholder named `name`.
    pub fn placeholder(name: impl Into<String>) -> Self {
        Self::Placeholder(PlaceholderMessage { name: name.into() })
    }
}

impl From<ChatMessage> for ChatPromptMessage {
    fn from(message: ChatMessage) -> Self {
        Self::Message(message)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CreatePromptRequest {
    #[serde(rename_all = "camelCase")]
    Chat {
        name: String,
        prompt: Vec<ChatPromptMessage>,
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
