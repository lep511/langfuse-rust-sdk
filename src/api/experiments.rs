//! Manage experiments and their runs.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::experiments::{Experiment, ExperimentItem};
use crate::pagination::{CursorPaginated, Pages, impl_cursor_query, paginate_cursor};

#[derive(Clone)]
pub struct ExperimentsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Query of `GET /api/public/experiments`. `from_start_time` is required.
/// Multi-value filters (`id`, `name`, `dataset_id`) are comma-separated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListExperimentsParams {
    /// Comma-separated field groups: `core`, `metadata`, `scores` (default `core`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
    /// Max 100, default 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    /// Scores per experiment when `fields` includes `scores` (max 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// ISO 8601 lower bound of experiment activity.
    pub from_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// JSON-encoded array of filter conditions on `id`, `name`, `datasetId`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl_cursor_query!(ListExperimentsParams);

impl ListExperimentsParams {
    pub fn new(from_start_time: impl Into<String>) -> Self {
        Self {
            fields: None,
            limit: None,
            score_limit: None,
            cursor: None,
            from_start_time: from_start_time.into(),
            to_start_time: None,
            id: None,
            name: None,
            dataset_id: None,
            filter: None,
        }
    }
}

/// Query of `GET /api/public/experiment-items`. `from_start_time` is required.
/// Multi-value filters are comma-separated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListExperimentItemsParams {
    /// Comma-separated field groups: `core`, `dataset`, `io`, `metadata`,
    /// `itemMetadata`, `experimentMetadata`, `scores` (default `core,dataset`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub from_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl_cursor_query!(ListExperimentItemsParams);

impl ListExperimentItemsParams {
    pub fn new(from_start_time: impl Into<String>) -> Self {
        Self {
            fields: None,
            limit: None,
            score_limit: None,
            cursor: None,
            from_start_time: from_start_time.into(),
            to_start_time: None,
            experiment_id: None,
            experiment_name: None,
            experiment_item_id: None,
            dataset_id: None,
            filter: None,
        }
    }
}

impl ExperimentsClient {
    /// `GET /api/public/experiments`, latest activity first (cursor-paginated).
    pub async fn list(
        &self,
        params: &ListExperimentsParams,
    ) -> Result<CursorPaginated<Experiment>, LangfuseError> {
        self.http
            .get_with_query("/api/public/experiments", params)
            .await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.cursor`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(
        &self,
        params: ListExperimentsParams,
    ) -> Pages<'_, CursorPaginated<Experiment>> {
        paginate_cursor(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    /// `GET /api/public/experiment-items`, newest first (cursor-paginated).
    pub async fn list_items(
        &self,
        params: &ListExperimentItemsParams,
    ) -> Result<CursorPaginated<ExperimentItem>, LangfuseError> {
        self.http
            .get_with_query("/api/public/experiment-items", params)
            .await
    }

    /// Lazily stream every page of [`list_items`](Self::list_items), following `meta.cursor`;
    /// see [`pagination`](crate::pagination).
    pub fn list_items_pages(
        &self,
        params: ListExperimentItemsParams,
    ) -> Pages<'_, CursorPaginated<ExperimentItem>> {
        paginate_cursor(params, move |params| async move {
            self.list_items(&params).await
        })
    }
}
