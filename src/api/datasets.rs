//! Create, list, get and delete datasets and dataset runs.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::datasets::{CreateDatasetRequest, Dataset};
use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};

#[derive(Clone)]
pub struct DatasetsClient {
    pub(crate) http: Arc<HttpClient>,
}

/// Options of [`LangfuseClient::get_dataset`](crate::LangfuseClient::get_dataset).
///
/// Build it from [`GetDatasetOptions::default`] with the `with_*` methods.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GetDatasetOptions {
    /// Items fetched per request (default: 50).
    pub fetch_items_page_size: i64,
    /// Read the items as they were at this time (default: the latest version).
    pub version: Option<DateTime<Utc>>,
}

impl Default for GetDatasetOptions {
    fn default() -> Self {
        Self {
            fetch_items_page_size: 50,
            version: None,
        }
    }
}

impl GetDatasetOptions {
    /// Set [`fetch_items_page_size`](Self::fetch_items_page_size).
    pub fn with_fetch_items_page_size(mut self, fetch_items_page_size: i64) -> Self {
        self.fetch_items_page_size = fetch_items_page_size;
        self
    }

    /// Set [`version`](Self::version).
    pub fn with_version(mut self, version: DateTime<Utc>) -> Self {
        self.version = Some(version);
        self
    }
}

impl DatasetsClient {
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<Dataset>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v2/datasets", params)
            .await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: PageParams) -> Pages<'_, Paginated<Dataset>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    pub async fn create(&self, req: &CreateDatasetRequest) -> Result<Dataset, LangfuseError> {
        self.http.post("/api/public/v2/datasets", req).await
    }

    pub async fn get(&self, dataset_name: &str) -> Result<Dataset, LangfuseError> {
        self.http
            .get(&format!(
                "/api/public/v2/datasets/{}",
                encode_path_segment(dataset_name)
            ))
            .await
    }
}
