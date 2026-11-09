use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::traces::{DeleteTraceResponse, DeleteTracesRequest};

pub struct TracesClient {
    pub(crate) http: Arc<HttpClient>,
}

impl TracesClient {
    pub async fn delete(&self, trace_id: &str) -> Result<DeleteTraceResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!("/api/public/traces/{trace_id}"))
            .await
    }

    pub async fn delete_batch(&self, req: &DeleteTracesRequest) -> Result<(), LangfuseError> {
        self.http
            .delete_with_body("/api/public/traces", req)
            .await
    }
}
