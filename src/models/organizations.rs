use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

wire_enum!(MembershipRole, "SCREAMING_SNAKE_CASE", {
    Owner,
    Admin,
    Member,
    Viewer,
});

/// Body of the membership upsert endpoints (`PUT .../memberships`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipRequest {
    pub user_id: String,
    pub role: MembershipRole,
}

/// Body of the membership delete endpoints (`DELETE .../memberships`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMembershipRequest {
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipResponse {
    pub user_id: String,
    pub role: MembershipRole,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipsResponse {
    pub memberships: Vec<MembershipResponse>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipDeletionResponse {
    pub message: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationProject {
    pub id: String,
    pub name: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationProjectsResponse {
    pub projects: Vec<OrganizationProject>,
}

/// An organization-scoped API key (secret not included).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationApiKey {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub name: Option<String>,
    /// Deprecated alias of `name`.
    pub note: Option<String>,
    pub public_key: String,
    pub display_secret_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationApiKeysResponse {
    pub api_keys: Vec<OrganizationApiKey>,
}
