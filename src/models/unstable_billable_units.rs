//! Billable units (unstable API, feature `unstable`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Bucket granularity for billable units queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BillableUnitsGranularity {
    Hour,
    Day,
    Week,
    Month,
}

/// Billable units of one project in one bucket.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillableUnitsRow {
    /// Start of the UTC bucket.
    pub bucket: DateTime<Utc>,
    pub project_id: String,
    /// All billable units, the sum of the per-type counts.
    pub count: i64,
    pub traces: i64,
    pub observations: i64,
    pub scores: i64,
    pub topic_facet_summaries: i64,
}

/// Response of `GET /api/public/unstable/organizations/billable-units`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillableUnitsResponse {
    pub data: Vec<BillableUnitsRow>,
    pub usage_recorded_from: Option<DateTime<Utc>>,
}
