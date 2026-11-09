use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::models_api::{CreateModelRequest, Model};
use crate::pagination::{PageParams, Paginated};

pub struct ModelsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl ModelsClient {
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<Model>, LangfuseError> {
        self.http
            .get_with_query("/api/public/models", params)
            .await
    }

    pub async fn create(&self, req: &CreateModelRequest) -> Result<Model, LangfuseError> {
        self.http.post("/api/public/models", req).await
    }

    pub async fn get(&self, id: &str) -> Result<Model, LangfuseError> {
        self.http.get(&format!("/api/public/models/{id}")).await
    }

    pub async fn update(&self, id: &str, req: &CreateModelRequest) -> Result<Model, LangfuseError> {
        self.http.put(&format!("/api/public/models/{id}"), req).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content(&format!("/api/public/models/{id}"))
            .await
    }
}
