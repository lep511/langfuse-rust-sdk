//! Batch ingestion of traces, spans, generations, scores and events.

use std::sync::Arc;

use reqwest::header::{HeaderMap, HeaderValue};

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::ingestion::IngestTracesRequest;

/// Ingestion data-model version the API requires from direct OTLP exporters.
const INGESTION_VERSION: &str = "4";

#[derive(Clone)]
pub struct IngestionClient {
    pub(crate) http: Arc<HttpClient>,
}

impl IngestionClient {
    /// `POST /api/public/otel/v1/traces` (OTLP/HTTP JSON).
    ///
    /// Sends `x-langfuse-ingestion-version: 4`, as the API requires.
    pub async fn ingest_traces(&self, req: &IngestTracesRequest) -> Result<(), LangfuseError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-langfuse-ingestion-version",
            HeaderValue::from_static(INGESTION_VERSION),
        );
        self.http
            .post_no_response_with_headers("/api/public/otel/v1/traces", req, &headers)
            .await
    }
}
