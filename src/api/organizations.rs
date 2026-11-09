//! Organization-level operations.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::organizations::{
    DeleteMembershipRequest, MembershipDeletionResponse, MembershipRequest, MembershipResponse,
    MembershipsResponse, OrganizationApiKeysResponse, OrganizationProjectsResponse,
};

const ORG_MEMBERSHIPS: &str = "/api/public/organizations/memberships";

/// Organization administration. Every endpoint requires an organization-scoped API key.
pub struct OrganizationsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl OrganizationsClient {
    /// `GET /api/public/organizations/memberships`
    pub async fn get_memberships(&self) -> Result<MembershipsResponse, LangfuseError> {
        self.http.get(ORG_MEMBERSHIPS).await
    }

    /// `PUT /api/public/organizations/memberships` (create or update).
    pub async fn update_membership(
        &self,
        req: &MembershipRequest,
    ) -> Result<MembershipResponse, LangfuseError> {
        self.http.put(ORG_MEMBERSHIPS, req).await
    }

    /// `DELETE /api/public/organizations/memberships` with `{userId}`.
    pub async fn delete_membership(
        &self,
        user_id: &str,
    ) -> Result<MembershipDeletionResponse, LangfuseError> {
        let body = DeleteMembershipRequest {
            user_id: user_id.to_string(),
        };
        self.http
            .delete_with_body_response(ORG_MEMBERSHIPS, &body)
            .await
    }

    /// `GET /api/public/projects/{projectId}/memberships`
    pub async fn get_project_memberships(
        &self,
        project_id: &str,
    ) -> Result<MembershipsResponse, LangfuseError> {
        self.http.get(&project_memberships_path(project_id)).await
    }

    /// `PUT /api/public/projects/{projectId}/memberships`. The user must already
    /// belong to the organization.
    pub async fn update_project_membership(
        &self,
        project_id: &str,
        req: &MembershipRequest,
    ) -> Result<MembershipResponse, LangfuseError> {
        self.http
            .put(&project_memberships_path(project_id), req)
            .await
    }

    /// `DELETE /api/public/projects/{projectId}/memberships` with `{userId}`.
    pub async fn delete_project_membership(
        &self,
        project_id: &str,
        user_id: &str,
    ) -> Result<MembershipDeletionResponse, LangfuseError> {
        let body = DeleteMembershipRequest {
            user_id: user_id.to_string(),
        };
        self.http
            .delete_with_body_response(&project_memberships_path(project_id), &body)
            .await
    }

    /// `GET /api/public/organizations/projects`
    pub async fn get_projects(&self) -> Result<OrganizationProjectsResponse, LangfuseError> {
        self.http.get("/api/public/organizations/projects").await
    }

    /// `GET /api/public/organizations/apiKeys`
    pub async fn get_api_keys(&self) -> Result<OrganizationApiKeysResponse, LangfuseError> {
        self.http.get("/api/public/organizations/apiKeys").await
    }
}

fn project_memberships_path(project_id: &str) -> String {
    format!(
        "/api/public/projects/{}/memberships",
        encode_path_segment(project_id)
    )
}
