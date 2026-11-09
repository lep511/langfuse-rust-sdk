//! Submit end-user feedback.

use std::sync::Arc;

pub use crate::models::feedback::{
    FeedbackTargetType, SubmitFeedbackRequest, SubmitFeedbackResponse,
};

use crate::error::LangfuseError;
use crate::http::HttpClient;

#[derive(Clone)]
pub struct FeedbackClient {
    pub(crate) http: Arc<HttpClient>,
}

impl FeedbackClient {
    /// `POST /api/public/feedback` (201). Not available in the HIPAA cloud region.
    pub async fn submit(
        &self,
        req: &SubmitFeedbackRequest,
    ) -> Result<SubmitFeedbackResponse, LangfuseError> {
        self.http.post("/api/public/feedback", req).await
    }
}
