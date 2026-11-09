//! Health-check endpoint.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::health::HealthResponse;

#[derive(Clone)]
pub struct HealthClient {
    pub(crate) http: Arc<HttpClient>,
}

impl HealthClient {
    pub async fn check(&self) -> Result<HealthResponse, LangfuseError> {
        self.http.get("/api/public/health").await
    }
}
