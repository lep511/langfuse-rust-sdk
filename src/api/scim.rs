//! SCIM 2.0 user provisioning.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::scim::{
    CreateScimUserRequest, ResourceTypesResponse, SchemasResponse, ScimUser, ScimUsersListResponse,
    ServiceProviderConfig,
};

const BASE: &str = "/api/public/scim";

/// SCIM 2.0 user provisioning. Every endpoint requires an organization-scoped API key.
#[derive(Clone)]
pub struct ScimClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListScimUsersParams {
    /// SCIM filter expression, e.g. `userName eq "ada@example.com"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// 1-based index of the first result (default 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i64>,
    /// Maximum results (default 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
}

impl ScimClient {
    /// `GET /api/public/scim/ServiceProviderConfig`
    pub async fn get_service_provider_config(
        &self,
    ) -> Result<ServiceProviderConfig, LangfuseError> {
        self.http
            .get(&format!("{BASE}/ServiceProviderConfig"))
            .await
    }

    /// `GET /api/public/scim/ResourceTypes`
    pub async fn get_resource_types(&self) -> Result<ResourceTypesResponse, LangfuseError> {
        self.http.get(&format!("{BASE}/ResourceTypes")).await
    }

    /// `GET /api/public/scim/Schemas`
    pub async fn get_schemas(&self) -> Result<SchemasResponse, LangfuseError> {
        self.http.get(&format!("{BASE}/Schemas")).await
    }

    /// `GET /api/public/scim/Users`
    pub async fn list_users(
        &self,
        params: &ListScimUsersParams,
    ) -> Result<ScimUsersListResponse, LangfuseError> {
        self.http
            .get_with_query(&format!("{BASE}/Users"), params)
            .await
    }

    /// `POST /api/public/scim/Users`
    pub async fn create_user(
        &self,
        req: &CreateScimUserRequest,
    ) -> Result<ScimUser, LangfuseError> {
        self.http.post(&format!("{BASE}/Users"), req).await
    }

    /// `GET /api/public/scim/Users/{userId}`
    pub async fn get_user(&self, user_id: &str) -> Result<ScimUser, LangfuseError> {
        self.http.get(&user_path(user_id)).await
    }

    /// `DELETE /api/public/scim/Users/{userId}`: removes the user from the
    /// organization (the user entity itself is kept).
    pub async fn delete_user(&self, user_id: &str) -> Result<(), LangfuseError> {
        self.http.delete_no_content(&user_path(user_id)).await
    }
}

fn user_path(user_id: &str) -> String {
    format!("{BASE}/Users/{}", encode_path_segment(user_id))
}
