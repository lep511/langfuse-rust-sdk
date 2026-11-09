//! Manage individual prompt versions and their labels.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::prompts::Prompt;

#[derive(Clone)]
pub struct PromptVersionClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Body of `PATCH /api/public/v2/prompts/{name}/versions/{version}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptVersionRequest {
    /// New labels for the version. Labels are unique across versions; `latest` is reserved.
    pub new_labels: Vec<String>,
}

impl PromptVersionClient {
    /// Replace the labels of one prompt version.
    pub async fn update(
        &self,
        name: &str,
        version: i64,
        req: &UpdatePromptVersionRequest,
    ) -> Result<Prompt, LangfuseError> {
        self.http
            .patch(
                &format!(
                    "/api/public/v2/prompts/{}/versions/{version}",
                    encode_path_segment(name)
                ),
                req,
            )
            .await
    }
}
