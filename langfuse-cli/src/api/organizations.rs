//! `langfuse api organizations` (organization-scoped key) and the membership
//! actions shared with `langfuse api projects`.

use clap::Args;
use langfuse_rust_sdk::models::organizations::{
    DeleteMembershipRequest, MembershipRequest, MembershipRole,
};

use crate::api::llm_connections::{DeleteOrganization, ListOrganization, UpsertOrganization};
use crate::args::{BodyChannel, wire_enum};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

const ROLES: [&str; 4] = ["OWNER", "ADMIN", "MEMBER", "VIEWER"];

/// `--user-id` / `--role` of membership upserts.
#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
pub struct MembershipFields {
    /// The user
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    user_id: Option<String>,
    /// The role
    #[arg(long, value_parser = ROLES, required_unless_present_any = ["body_json", "body_file"])]
    role: Option<String>,
}

/// A membership upsert body.
#[derive(Debug, Args)]
pub struct MembershipBody {
    #[command(flatten)]
    fields: MembershipFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl MembershipBody {
    /// Add the body to `request`.
    pub fn apply(
        &self,
        request: PreparedRequest,
        ctx: &mut Context<'_>,
    ) -> Result<PreparedRequest> {
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        request.body_from(&MembershipRequest {
            user_id: self.fields.user_id.clone().unwrap_or_default(),
            role: wire_enum::<MembershipRole>(
                "role",
                self.fields.role.as_deref().unwrap_or_default(),
            )?,
        })
    }
}

/// `--user-id` of membership deletions.
#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
pub struct UserFields {
    /// The user
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    user_id: Option<String>,
}

/// A membership deletion body.
#[derive(Debug, Args)]
pub struct UserBody {
    #[command(flatten)]
    fields: UserFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl UserBody {
    /// Add the body to `request`.
    pub fn apply(
        &self,
        request: PreparedRequest,
        ctx: &mut Context<'_>,
    ) -> Result<PreparedRequest> {
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        request.body_from(&DeleteMembershipRequest {
            user_id: self.fields.user_id.clone().unwrap_or_default(),
        })
    }
}

path_op! {
    ListMemberships {}
    meta = OperationMeta::new(
        "organizations", "list-memberships", "organizations_getOrganizationMemberships",
        HttpMethod::Get, "/api/public/organizations/memberships",
    )
    .columns(&["userId", "role", "email", "name"]);
}

#[derive(Debug, Args)]
pub struct UpdateMembership {
    #[command(flatten)]
    body: MembershipBody,
}

impl Operation for UpdateMembership {
    const META: OperationMeta = OperationMeta::new(
        "organizations",
        "update-membership",
        "organizations_updateOrganizationMembership",
        HttpMethod::Put,
        "/api/public/organizations/memberships",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.body.apply(PreparedRequest::new(&Self::META), ctx)
    }
}

#[derive(Debug, Args)]
pub struct DeleteMembership {
    #[command(flatten)]
    body: UserBody,
}

impl Operation for DeleteMembership {
    const META: OperationMeta = OperationMeta::new(
        "organizations",
        "delete-membership",
        "organizations_deleteOrganizationMembership",
        HttpMethod::Delete,
        "/api/public/organizations/memberships",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.body.apply(PreparedRequest::new(&Self::META), ctx)
    }
}

path_op! {
    ListProjects {}
    meta = OperationMeta::new(
        "organizations", "list-projects", "organizations_getOrganizationProjects",
        HttpMethod::Get, "/api/public/organizations/projects",
    )
    .columns(&["id", "name", "retentionDays", "createdAt"]);
}

path_op! {
    ListApiKeys {}
    meta = OperationMeta::new(
        "organizations", "list-api-keys", "organizations_getOrganizationApiKeys",
        HttpMethod::Get, "/api/public/organizations/apiKeys",
    )
    .columns(&["id", "publicKey", "displaySecretKey", "note", "expiresAt", "lastUsedAt"]);
}

resource! {
    pub enum Command {
        /// Get the organization's memberships
        ListMemberships(ListMemberships),
        /// Create or update a membership of the organization
        UpdateMembership(UpdateMembership),
        /// Delete a membership of the organization
        DeleteMembership(DeleteMembership),
        /// Get the organization's projects
        ListProjects(ListProjects),
        /// Get the organization's API keys
        ListApiKeys(ListApiKeys),
        /// Get the organization-level LLM connections
        ListLlmConnections(ListOrganization),
        /// Create or update an organization-level LLM connection (upserted on --provider;
        /// inherited by projects without an override)
        UpsertLlmConnection(UpsertOrganization),
        /// Delete an organization-level LLM connection
        DeleteLlmConnection(DeleteOrganization),
    }
}
