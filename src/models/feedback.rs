//! Feedback request and response types.

use serde::{Deserialize, Serialize};

/// What the feedback is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackTargetType {
    Skill,
    McpTool,
    Cli,
    Docs,
    PublicApi,
    Other,
}

/// Body of `POST /api/public/feedback`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitFeedbackRequest {
    pub target_type: FeedbackTargetType,
    /// 1–200 characters, e.g. a skill name or endpoint.
    pub target: String,
    /// 1–3000 characters.
    pub feedback: String,
    /// 1–1500 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitFeedbackResponse {
    pub id: String,
}
