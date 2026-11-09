//! SCIM 2.0 types. Field names follow SCIM, including the capitalized `Resources`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProviderConfig {
    pub schemas: Vec<String>,
    pub documentation_uri: String,
    pub patch: ScimFeatureSupport,
    pub bulk: BulkConfig,
    pub filter: FilterConfig,
    pub change_password: ScimFeatureSupport,
    pub sort: ScimFeatureSupport,
    pub etag: ScimFeatureSupport,
    pub authentication_schemes: Vec<AuthenticationScheme>,
    pub meta: ResourceMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimFeatureSupport {
    pub supported: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkConfig {
    pub supported: bool,
    pub max_operations: i64,
    pub max_payload_size: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterConfig {
    pub supported: bool,
    pub max_results: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMeta {
    pub resource_type: String,
    pub location: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationScheme {
    pub name: String,
    pub description: String,
    pub spec_uri: String,
    pub r#type: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTypesResponse {
    pub schemas: Vec<String>,
    pub total_results: i64,
    #[serde(rename = "Resources")]
    pub resources: Vec<ResourceType>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceType {
    pub schemas: Option<Vec<String>>,
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub description: String,
    pub schema: String,
    pub schema_extensions: Vec<SchemaExtension>,
    pub meta: ResourceMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaExtension {
    pub schema: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemasResponse {
    pub schemas: Vec<String>,
    pub total_results: i64,
    #[serde(rename = "Resources")]
    pub resources: Vec<SchemaResource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaResource {
    pub id: String,
    pub name: String,
    pub description: String,
    /// SCIM attribute definitions, kept as raw JSON.
    pub attributes: Vec<serde_json::Value>,
    pub meta: ResourceMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUsersListResponse {
    pub schemas: Vec<String>,
    pub total_results: i64,
    /// 1-based index of the first result.
    pub start_index: i64,
    pub items_per_page: i64,
    #[serde(rename = "Resources")]
    pub resources: Vec<ScimUser>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUser {
    pub schemas: Vec<String>,
    pub id: String,
    pub user_name: String,
    pub name: ScimName,
    pub emails: Vec<ScimEmail>,
    pub meta: UserMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMeta {
    pub resource_type: String,
    /// Timestamp string as returned by the server.
    pub created: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimName {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimEmail {
    pub primary: bool,
    pub value: String,
    pub r#type: String,
}

/// Body of `POST /api/public/scim/Users`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScimUserRequest {
    /// The user's email address.
    pub user_name: String,
    pub name: ScimName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emails: Option<Vec<ScimEmail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// Deprecated and ignored by the server; accepted for identity-provider compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}
