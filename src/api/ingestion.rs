//! Batch ingestion of traces, spans, generations, scores and events.

use std::sync::Arc;

use reqwest::header::{HeaderMap, HeaderValue};

use crate::error::LangfuseError;
use crate::http::{CallOptions, HttpClient};
use crate::models::ingestion::IngestTracesRequest;

/// Ingestion data-model version the API requires from direct OTLP exporters.
const INGESTION_VERSION: &str = "4";

/// Path of the OTLP/HTTP traces endpoint.
pub(crate) const OTEL_TRACES_PATH: &str = "/api/public/otel/v1/traces";

#[derive(Clone)]
pub struct IngestionClient {
    pub(crate) http: Arc<HttpClient>,
}

impl IngestionClient {
    /// `POST /api/public/otel/v1/traces` (OTLP/HTTP JSON).
    ///
    /// Sends `x-langfuse-ingestion-version: 4`, as the API requires. Retried
    /// like an idempotent request (also on 5xx and timeouts): spans carry their
    /// IDs, so a repeated export does not duplicate them.
    pub async fn ingest_traces(&self, req: &IngestTracesRequest) -> Result<(), LangfuseError> {
        self.post_traces(req, OTEL_TRACES_PATH, CallOptions::default())
            .await
    }

    /// [`ingest_traces`](Self::ingest_traces) to `path` (relative to the base
    /// URL), gzip-compressed when `gzip` is set; used by the tracer's exporter.
    #[cfg(feature = "otel")]
    pub(crate) async fn export_traces(
        &self,
        req: &IngestTracesRequest,
        path: &str,
        gzip: bool,
    ) -> Result<(), LangfuseError> {
        let options = CallOptions {
            gzip,
            ..CallOptions::default()
        };
        self.post_traces(req, path, options).await
    }

    async fn post_traces(
        &self,
        req: &IngestTracesRequest,
        path: &str,
        options: CallOptions<'_>,
    ) -> Result<(), LangfuseError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-langfuse-ingestion-version",
            HeaderValue::from_static(INGESTION_VERSION),
        );
        self.http
            .post_no_response_with_options(
                path,
                req,
                CallOptions {
                    headers: Some(&headers),
                    idempotent: Some(true),
                    ..options
                },
            )
            .await
    }
}
