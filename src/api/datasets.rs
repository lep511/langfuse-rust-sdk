//! Create, list, get and delete datasets and dataset runs.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::datasets::{CreateDatasetRequest, Dataset};
use crate::pagination::{PageParams, Paginated};

pub struct DatasetsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl DatasetsClient {
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<Dataset>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v2/datasets", params)
            .await
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
