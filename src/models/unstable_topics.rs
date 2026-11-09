//! Topics (unstable API in closed beta, feature `unstable`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::evaluation_common::EvaluationRuleReadFilter;
use crate::pagination::PageMeta;

/// Sort order of `GET /api/public/unstable/topics` (default `traceCount.desc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TopicsOrderBy {
    #[serde(rename = "traceCount.desc")]
    TraceCountDesc,
    #[serde(rename = "traceCount.asc")]
    TraceCountAsc,
    #[serde(rename = "name.asc")]
    NameAsc,
    #[serde(rename = "name.desc")]
    NameDesc,
}

/// Sort order of summaries and assignments (default `timestamp.desc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TopicResultsOrderBy {
    #[serde(rename = "timestamp.desc")]
    TimestampDesc,
    #[serde(rename = "timestamp.asc")]
    TimestampAsc,
}

wire_enum!(TopicSummaryState, "snake_case", {
    Complete,
    NotApplicable,
    InsufficientInput,
});

wire_enum!(TopicAssignmentStatus, "lowercase", {
    Assigned,
    Outlier,
});

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicFacet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_built_in: bool,
    /// Whether a Topics rule runs the facet (true for all facets without rules).
    pub active: bool,
    pub rule_ids: Vec<String>,
    /// Latest facet version.
    pub version: i64,
    /// Prompt of the latest version.
    pub prompt: String,
    /// Run that produced the current map; `None` before the first map.
    pub published_run: Option<TopicFacetPublishedRun>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicFacetPublishedRun {
    pub id: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub topic_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicFacetRef {
    pub id: String,
    pub name: String,
    pub version: i64,
}

/// Time window (trace start time; `from` inclusive, `to` exclusive).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicWindow {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    /// Stable id, kept across map updates.
    pub id: String,
    pub version_id: String,
    pub name: String,
    pub description: String,
    pub facet: TopicFacetRef,
    /// `None` when the project has no rule.
    pub rule_id: Option<String>,
    pub run_id: String,
    /// Traces in the window currently assigned to this topic.
    pub trace_count: i64,
}

/// Response of `GET /api/public/unstable/topics/{topicId}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicWithStatus {
    #[serde(flatten)]
    pub topic: Topic,
    /// Always true; retired topics are not returned.
    pub in_current_map: bool,
    pub window: TopicWindow,
}

/// Traces in the window by their current result for one facet.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicStatusCounts {
    pub assigned: i64,
    pub outlier: i64,
    pub awaiting_map: i64,
    pub not_applicable: i64,
    pub insufficient_input: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicFacetCounts {
    pub facet: TopicFacetRef,
    pub rule_id: Option<String>,
    pub published_run_id: Option<String>,
    pub counts: TopicStatusCounts,
}

/// Response of `GET /api/public/unstable/topics`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicList {
    pub window: TopicWindow,
    pub data: Vec<Topic>,
    /// Per-facet trace counts by status.
    pub facets: Vec<TopicFacetCounts>,
    pub meta: PageMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicRule {
    pub id: String,
    pub name: String,
    /// Observation filter selecting the rule's traces, returned verbatim.
    pub filter: Vec<EvaluationRuleReadFilter>,
    /// Fraction of matching traces processed, between 0 and 1.
    pub sampling: f64,
    pub facet_ids: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicSummary {
    pub trace_id: String,
    pub trace_name: String,
    pub environment: String,
    /// Trace start time.
    pub timestamp: DateTime<Utc>,
    pub facet_id: String,
    pub facet_name: String,
    pub facet_version: i64,
    pub state: TopicSummaryState,
    /// Set when `state` is `complete`.
    pub summary: Option<String>,
    pub summary_truncated: bool,
    pub processed_at: DateTime<Utc>,
}

/// Cursor metadata of summaries and assignments. Keep fetching until `cursor` is
/// `None`: a short (even empty) page may still have a cursor.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicResultListMeta {
    pub cursor: Option<String>,
    /// Set when the page is shorter than `limit` but more rows may follow.
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicSummaryList {
    pub window: TopicWindow,
    pub data: Vec<TopicSummary>,
    pub meta: TopicResultListMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicAssignmentTopic {
    pub id: String,
    pub version_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicAssignment {
    pub trace_id: String,
    /// Trace start time.
    pub timestamp: DateTime<Utc>,
    pub facet_id: String,
    pub facet_name: String,
    pub facet_version: i64,
    pub rule_id: Option<String>,
    pub status: TopicAssignmentStatus,
    /// Set when `status` is `assigned`.
    pub topic: Option<TopicAssignmentTopic>,
    /// `None` for assignments made as traces arrive.
    pub run_id: Option<String>,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicAssignmentList {
    pub window: TopicWindow,
    pub data: Vec<TopicAssignment>,
    pub meta: TopicResultListMeta,
}
