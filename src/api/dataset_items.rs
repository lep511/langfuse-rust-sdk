use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::dataset_items::{CreateDatasetItemRequest, DatasetItem};
use crate::pagination::Paginated;

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

impl DatasetItemsClient {
    pub async fn list(
        &self,
        params: &ListDatasetItemsParams,
    ) -> Result<Paginated<DatasetItem>, LangfuseError> {
        self.http
            .get_with_query("/api/public/dataset-items", params)
            .await
    }

    pub async fn create(
        &self,
        req: &CreateDatasetItemRequest,
    ) -> Result<DatasetItem, LangfuseError> {
        self.http.post("/api/public/dataset-items", req).await
    }

    pub async fn get(&self, id: &str) -> Result<DatasetItem, LangfuseError> {
        self.http
            .get(&format!("/api/public/dataset-items/{id}"))
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/dataset-items/{id}"))
            .await
    }
}
