//! Query aggregated metrics.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::metrics::{MetricsQuery, MetricsResponse};

#[derive(Clone)]
pub struct MetricsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Serialize)]
struct MetricsQueryParam {
    query: String,
}

impl MetricsClient {
    /// `GET /api/public/v2/metrics?query=<JSON>`
    pub async fn query(&self, query: &MetricsQuery) -> Result<MetricsResponse, LangfuseError> {
        let param = MetricsQueryParam {
            query: serde_json::to_string(query)?,
        };
        self.http
            .get_with_query("/api/public/v2/metrics", &param)
            .await
    }
}
