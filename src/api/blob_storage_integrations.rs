//! Blob storage integration configuration.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::blob_storage_integrations::{
    BlobStorageIntegrationDeletionResponse, BlobStorageIntegrationResponse,
    BlobStorageIntegrationStatusResponse, BlobStorageIntegrationsResponse,
    CreateBlobStorageIntegrationRequest,
};

const BASE: &str = "/api/public/integrations/blob-storage";

/// Blob storage export integrations. Every endpoint requires an organization-scoped API key.
pub struct BlobStorageIntegrationsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl BlobStorageIntegrationsClient {
    /// `GET /api/public/integrations/blob-storage`
    pub async fn list(&self) -> Result<BlobStorageIntegrationsResponse, LangfuseError> {
        self.http.get(BASE).await
    }

    /// `PUT /api/public/integrations/blob-storage`: create or update the integration of a
    /// project. The server validates the configuration with a test upload.
    pub async fn upsert(
        &self,
        req: &CreateBlobStorageIntegrationRequest,
    ) -> Result<BlobStorageIntegrationResponse, LangfuseError> {
        self.http.put(BASE, req).await
    }

    /// `GET /api/public/integrations/blob-storage/{id}`: sync status.
    pub async fn get_status(
        &self,
        id: &str,
    ) -> Result<BlobStorageIntegrationStatusResponse, LangfuseError> {
        self.http.get(&integration_path(id)).await
    }

    /// `DELETE /api/public/integrations/blob-storage/{id}`
    pub async fn delete(
        &self,
        id: &str,
    ) -> Result<BlobStorageIntegrationDeletionResponse, LangfuseError> {
        self.http.delete_with_response(&integration_path(id)).await
    }
}

fn integration_path(id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(id))
}
