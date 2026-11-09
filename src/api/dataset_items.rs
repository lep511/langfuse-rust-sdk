//! Create, update, get and delete dataset items.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::dataset_items::{
    CreateDatasetItemRequest, DatasetItem, DeleteDatasetItemResponse,
};
use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};

#[derive(Clone)]
pub struct DatasetItemsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDatasetItemsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_observation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

impl_page_query!(ListDatasetItemsParams);

impl DatasetItemsClient {
    pub async fn list(
        &self,
        params: &ListDatasetItemsParams,
    ) -> Result<Paginated<DatasetItem>, LangfuseError> {
        self.http
            .get_with_query("/api/public/dataset-items", params)
            .await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: ListDatasetItemsParams) -> Pages<'_, Paginated<DatasetItem>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    pub async fn create(
        &self,
        req: &CreateDatasetItemRequest,
    ) -> Result<DatasetItem, LangfuseError> {
        self.http.post("/api/public/dataset-items", req).await
    }

    pub async fn get(&self, id: &str) -> Result<DatasetItem, LangfuseError> {
        self.http
            .get(&format!(
                "/api/public/dataset-items/{}",
                encode_path_segment(id)
            ))
            .await
    }

    /// `DELETE /api/public/dataset-items/{id}`
    pub async fn delete(&self, id: &str) -> Result<DeleteDatasetItemResponse, LangfuseError> {
        self.http
            .delete_with_response(&format!(
                "/api/public/dataset-items/{}",
                encode_path_segment(id)
            ))
            .await
    }
}
