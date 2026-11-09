use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::ingestion::IngestTracesRequest;

pub struct IngestionClient {
    pub(crate) http: Arc<HttpClient>,
}

impl IngestionClient {
    pub async fn ingest_traces(&self, req: &IngestTracesRequest) -> Result<(), LangfuseError> {
        self.http
            .post_no_response("/api/public/otel/v1/traces", req)
            .await
    }
}
