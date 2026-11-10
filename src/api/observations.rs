//! Query observations (spans, generations, events).

use std::sync::Arc;
use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::error::LangfuseError;
use crate::http::{CallOptions, HttpClient};
use crate::models::observations::ObservationV2;
use crate::pagination::{CursorPaginated, Pages, impl_cursor_query, paginate_cursor};

/// Timeout of each page request of [`ObservationsClient::list_pages`] (the
/// Python SDK's `iter_observation_pages`): the server scans for up to 45 s.
pub(crate) const PAGE_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct ObservationsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Query parameters of `GET /api/public/v2/observations`.
///
/// Serialized as a list of `(key, value)` pairs so that `environment` can be
/// repeated once per value (`environment=a&environment=b`), as the API expects.
#[derive(Debug, Clone, Default)]
pub struct ListObservationsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub r#type: Option<String>,
    pub trace_id: Option<String>,
    pub level: Option<String>,
    pub parent_observation_id: Option<String>,
    /// Matches any of the given environments.
    pub environment: Option<Vec<String>>,
    pub from_start_time: Option<String>,
    pub to_start_time: Option<String>,
    pub version: Option<String>,
    /// Comma-separated field groups to include (e.g. `core,basic,usage`).
    pub fields: Option<String>,
    pub expand_metadata: Option<String>,
    pub parse_io_as_json: Option<bool>,
    pub is_root_observation: Option<bool>,
    /// JSON-encoded array of filter conditions.
    pub filter: Option<String>,
}

impl_cursor_query!(ListObservationsParams);

impl ListObservationsParams {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        fn push<T: ToString>(
            pairs: &mut Vec<(&'static str, String)>,
            key: &'static str,
            value: &Option<T>,
        ) {
            if let Some(v) = value {
                pairs.push((key, v.to_string()));
            }
        }

        let mut pairs = Vec::new();
        push(&mut pairs, "limit", &self.limit);
        push(&mut pairs, "cursor", &self.cursor);
        push(&mut pairs, "name", &self.name);
        push(&mut pairs, "userId", &self.user_id);
        push(&mut pairs, "sessionId", &self.session_id);
        push(&mut pairs, "type", &self.r#type);
        push(&mut pairs, "traceId", &self.trace_id);
        push(&mut pairs, "level", &self.level);
        push(
            &mut pairs,
            "parentObservationId",
            &self.parent_observation_id,
        );
        for env in self.environment.iter().flatten() {
            pairs.push(("environment", env.clone()));
        }
        push(&mut pairs, "fromStartTime", &self.from_start_time);
        push(&mut pairs, "toStartTime", &self.to_start_time);
        push(&mut pairs, "version", &self.version);
        push(&mut pairs, "fields", &self.fields);
        push(&mut pairs, "expandMetadata", &self.expand_metadata);
        push(&mut pairs, "parseIoAsJson", &self.parse_io_as_json);
        push(&mut pairs, "isRootObservation", &self.is_root_observation);
        push(&mut pairs, "filter", &self.filter);
        pairs
    }
}

impl Serialize for ListObservationsParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.to_pairs())
    }
}

impl ObservationsClient {
    /// `GET /api/public/v2/observations` (cursor-paginated).
    pub async fn list(
        &self,
        params: &ListObservationsParams,
    ) -> Result<CursorPaginated<ObservationV2>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v2/observations", params)
            .await
    }

    /// [`list`](Self::list) with per-request retries and timeout.
    pub(crate) async fn list_with_options(
        &self,
        params: &ListObservationsParams,
        options: CallOptions<'_>,
    ) -> Result<CursorPaginated<ObservationV2>, LangfuseError> {
        self.http
            .get_with_query_options("/api/public/v2/observations", params, options)
            .await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.cursor`;
    /// see [`pagination`](crate::pagination).
    ///
    /// Each page request has a 60 s timeout, as in the Python SDK, because the
    /// server can scan for up to 45 s; the
    /// [`RequestOptions::timeout`](crate::RequestOptions::timeout) of a
    /// [`with_options`](crate::LangfuseClient::with_options) view replaces it.
    pub fn list_pages(
        &self,
        params: ListObservationsParams,
    ) -> Pages<'_, CursorPaginated<ObservationV2>> {
        paginate_cursor(params, move |params| async move {
            let options = CallOptions {
                default_timeout: Some(PAGE_FETCH_TIMEOUT),
                ..CallOptions::default()
            };
            self.list_with_options(&params, options).await
        })
    }
}
