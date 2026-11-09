use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::prompts::Prompt;

pub struct PromptVersionClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptVersionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

impl PromptVersionClient {
    pub async fn get(&self, name: &str, version: i64) -> Result<Prompt, LangfuseError> {
        self.http
            .get(&format!("/api/public/v2/prompts/{name}/versions/{version}"))
            .await
    }

    pub async fn update(
        &self,
        name: &str,
        version: i64,
        req: &UpdatePromptVersionRequest,
    ) -> Result<Prompt, LangfuseError> {
        self.http
            .patch(
                &format!("/api/public/v2/prompts/{name}/versions/{version}"),
                req,
            )
            .await
    }
}
