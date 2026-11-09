//! CRUD operations on LLM model definitions.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::models_api::{CreateModelRequest, Model};
use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};

#[derive(Clone)]
pub struct ModelsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl ModelsClient {
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<Model>, LangfuseError> {
        self.http.get_with_query("/api/public/models", params).await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: PageParams) -> Pages<'_, Paginated<Model>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    pub async fn create(&self, req: &CreateModelRequest) -> Result<Model, LangfuseError> {
        self.http.post("/api/public/models", req).await
    }

    pub async fn get(&self, id: &str) -> Result<Model, LangfuseError> {
        self.http.get(&model_path(id)).await
    }

    /// `PUT /api/public/models/{id}` (upsert).
    pub async fn update(&self, id: &str, req: &CreateModelRequest) -> Result<Model, LangfuseError> {
        self.http.put(&model_path(id), req).await
    }

    /// `DELETE /api/public/models/{id}` (204).
    pub async fn delete(&self, id: &str) -> Result<(), LangfuseError> {
        self.http.delete_no_content(&model_path(id)).await
    }
}

fn model_path(id: &str) -> String {
    format!("/api/public/models/{}", encode_path_segment(id))
}
