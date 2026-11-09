//! Create, list, get and delete traces.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::traces::{DeleteTraceResponse, DeleteTracesRequest};

#[derive(Clone)]
pub struct TracesClient {
    pub(crate) http: Arc<HttpClient>,
}

impl TracesClient {
    /// `DELETE /api/public/traces/{traceId}`
    pub async fn delete(&self, trace_id: &str) -> Result<DeleteTraceResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!(
                "/api/public/traces/{}",
                encode_path_segment(trace_id)
            ))
            .await
    }

    /// `DELETE /api/public/traces` with `{traceIds: [...]}`.
    pub async fn delete_batch(
        &self,
        req: &DeleteTracesRequest,
    ) -> Result<DeleteTraceResponse, LangfuseError> {
        self.http
            .delete_with_body_response("/api/public/traces", req)
            .await
    }
}
