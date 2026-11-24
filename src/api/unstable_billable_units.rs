//! Organization billable units (unstable API, feature `unstable`).

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::unstable_billable_units::{BillableUnitsGranularity, BillableUnitsResponse};

#[derive(Clone)]
pub struct UnstableBillableUnitsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Query of `GET /api/public/unstable/organizations/billable-units`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBillableUnitsParams {
    /// Start of the range (inclusive), ISO 8601 with a time zone.
    pub from_timestamp: String,
    /// End of the range (exclusive), ISO 8601 with a time zone.
    pub to_timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granularity: Option<BillableUnitsGranularity>,
}

impl GetBillableUnitsParams {
    pub fn new(from_timestamp: impl Into<String>, to_timestamp: impl Into<String>) -> Self {
        Self {
            from_timestamp: from_timestamp.into(),
            to_timestamp: to_timestamp.into(),
            granularity: None,
        }
    }
}

impl UnstableBillableUnitsClient {
    /// `GET /api/public/unstable/organizations/billable-units`
    pub async fn get(
        &self,
        params: &GetBillableUnitsParams,
    ) -> Result<BillableUnitsResponse, LangfuseError> {
        self.http
            .get_with_query(
                "/api/public/unstable/organizations/billable-units",
                params,
            )
            .await
    }
}
