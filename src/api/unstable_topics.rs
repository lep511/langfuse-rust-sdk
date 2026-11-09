//! Topics (unstable API in closed beta, feature `unstable`).
//!
//! Every `filter` parameter is a JSON-encoded array of
//! [`DashboardWidgetFilter`](crate::models::unstable_common::DashboardWidgetFilter)
//! conditions, combined with AND. Windows apply to the trace start time, default
//! to the last 7 days and span at most 93 days.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::unstable_topics::{
    TopicAssignmentList, TopicFacet, TopicList, TopicResultsOrderBy, TopicRule, TopicSummaryList,
    TopicWithStatus, TopicsOrderBy,
};
use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};

#[derive(Clone)]
pub struct UnstableTopicsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Query of `GET /api/public/unstable/topic-facets`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTopicFacetsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    /// Max 100, default 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

impl_page_query!(ListTopicFacetsParams);

/// Query of `GET /api/public/unstable/topic-rules`.
pub type ListTopicRulesParams = ListTopicFacetsParams;

/// Query of `GET /api/public/unstable/topics`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTopicsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<TopicsOrderBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

/// Query of `GET /api/public/unstable/topics/{topicId}`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTopicParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
}

/// Query of `GET /api/public/unstable/topic-summaries` (cursor-paginated).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTopicSummariesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_summaries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<TopicResultsOrderBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Query of `GET /api/public/unstable/topic-assignments` (cursor-paginated).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTopicAssignmentsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<TopicResultsOrderBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl UnstableTopicsClient {
    /// `GET /api/public/unstable/topic-facets` (active facets unless `filter` says otherwise).
    pub async fn list_facets(
        &self,
        params: &ListTopicFacetsParams,
    ) -> Result<Paginated<TopicFacet>, LangfuseError> {
        self.http
            .get_with_query("/api/public/unstable/topic-facets", params)
            .await
    }

    /// Lazily stream every page of [`list_facets`](Self::list_facets), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_facets_pages(
        &self,
        params: ListTopicFacetsParams,
    ) -> Pages<'_, Paginated<TopicFacet>> {
        paginate_pages(params, move |params| async move {
            self.list_facets(&params).await
        })
    }

    /// `GET /api/public/unstable/topic-facets/{facetId}` (inactive facets included).
    pub async fn get_facet(&self, facet_id: &str) -> Result<TopicFacet, LangfuseError> {
        self.http
            .get(&format!(
                "/api/public/unstable/topic-facets/{}",
                encode_path_segment(facet_id)
            ))
            .await
    }

    /// `GET /api/public/unstable/topics`: topics of each active facet's live map.
    pub async fn list(&self, params: &ListTopicsParams) -> Result<TopicList, LangfuseError> {
        self.http
            .get_with_query("/api/public/unstable/topics", params)
            .await
    }

    /// `GET /api/public/unstable/topics/{topicId}`
    pub async fn get(
        &self,
        topic_id: &str,
        params: &GetTopicParams,
    ) -> Result<TopicWithStatus, LangfuseError> {
        self.http
            .get_with_query(
                &format!(
                    "/api/public/unstable/topics/{}",
                    encode_path_segment(topic_id)
                ),
                params,
            )
            .await
    }

    /// `GET /api/public/unstable/topic-rules`
    pub async fn list_rules(
        &self,
        params: &ListTopicRulesParams,
    ) -> Result<Paginated<TopicRule>, LangfuseError> {
        self.http
            .get_with_query("/api/public/unstable/topic-rules", params)
            .await
    }

    /// Lazily stream every page of [`list_rules`](Self::list_rules), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_rules_pages(
        &self,
        params: ListTopicRulesParams,
    ) -> Pages<'_, Paginated<TopicRule>> {
        paginate_pages(params, move |params| async move {
            self.list_rules(&params).await
        })
    }

    /// `GET /api/public/unstable/topic-rules/{ruleId}`
    pub async fn get_rule(&self, rule_id: &str) -> Result<TopicRule, LangfuseError> {
        self.http
            .get(&format!(
                "/api/public/unstable/topic-rules/{}",
                encode_path_segment(rule_id)
            ))
            .await
    }

    /// `GET /api/public/unstable/topic-summaries`
    pub async fn list_summaries(
        &self,
        params: &ListTopicSummariesParams,
    ) -> Result<TopicSummaryList, LangfuseError> {
        self.http
            .get_with_query("/api/public/unstable/topic-summaries", params)
            .await
    }

    /// `GET /api/public/unstable/topic-assignments`
    pub async fn list_assignments(
        &self,
        params: &ListTopicAssignmentsParams,
    ) -> Result<TopicAssignmentList, LangfuseError> {
        self.http
            .get_with_query("/api/public/unstable/topic-assignments", params)
            .await
    }
}
