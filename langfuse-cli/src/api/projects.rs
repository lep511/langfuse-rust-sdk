//! `langfuse api projects`.

use clap::Args;
use langfuse_rust_sdk::api::projects::{
    CreateApiKeyRequest, CreateProjectRequest, UpdateProjectRequest,
};
use serde_json::Value;

use crate::api::organizations::{MembershipBody, UserBody};
use crate::args::{BodyChannel, date_time, json_object};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

path_op! {
    List {}
    meta = OperationMeta::new(
        "projects", "list", "projects_get",
        HttpMethod::Get, "/api/public/projects",
    )
    .columns(&["id", "name", "organization.name", "retentionDays"]);
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct ProjectFields {
    /// Project name
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    name: Option<String>,
    /// Metadata as a JSON object
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    metadata: Option<Value>,
    /// Days to retain data: 0 (forever) or at least 3
    #[arg(long, value_name = "DAYS")]
    retention: Option<i64>,
}

#[derive(Debug, Args)]
pub struct Create {
    #[command(flatten)]
    fields: ProjectFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "projects",
        "create",
        "projects_create",
        HttpMethod::Post,
        "/api/public/projects",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        let retention = f
            .retention
            .ok_or_else(|| CliError::usage("Missing required body option(s): --retention"))?;
        request.body_from(&CreateProjectRequest {
            name: f.name.clone().unwrap_or_default(),
            metadata: f.metadata.clone(),
            retention,
        })
    }
}

#[derive(Debug, Args)]
pub struct Update {
    /// The project ID
    project_id: String,
    #[command(flatten)]
    fields: ProjectFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "projects",
        "update",
        "projects_update",
        HttpMethod::Put,
        "/api/public/projects/{projectId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("projectId", &self.project_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        request.body_from(&UpdateProjectRequest {
            name: f.name.clone().unwrap_or_default(),
            metadata: f.metadata.clone(),
            retention: f.retention,
        })
    }
}

path_op! {
    Delete {
        /// The project ID
        project_id => "projectId",
    }
    meta = OperationMeta::new(
        "projects", "delete", "projects_delete",
        HttpMethod::Delete, "/api/public/projects/{projectId}",
    );
}

path_op! {
    ListApiKeys {
        /// The project ID
        project_id => "projectId",
    }
    meta = OperationMeta::new(
        "projects", "list-api-keys", "projects_getApiKeys",
        HttpMethod::Get, "/api/public/projects/{projectId}/apiKeys",
    )
    .columns(&["id", "publicKey", "displaySecretKey", "note", "expiresAt", "lastUsedAt"]);
}

#[derive(Debug, Args)]
pub struct CreateApiKey {
    /// The project ID
    project_id: String,
    #[command(flatten)]
    fields: ApiKeyFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct ApiKeyFields {
    /// Name of the key (not together with --note)
    #[arg(long, conflicts_with = "note")]
    name: Option<String>,
    /// Deprecated alias of --name
    #[arg(long)]
    note: Option<String>,
    /// Expiry date-time in the future (RFC 3339); unset: never expires
    #[arg(long, value_parser = date_time)]
    expires_at: Option<String>,
    /// Predefined public key (pk-lf-…; needs --predefined-secret-key)
    #[arg(long, requires = "predefined_secret_key")]
    predefined_public_key: Option<String>,
    /// Predefined secret key (sk-lf-…; needs --predefined-public-key)
    #[arg(long, requires = "predefined_public_key")]
    predefined_secret_key: Option<String>,
}

impl Operation for CreateApiKey {
    const META: OperationMeta = OperationMeta::new(
        "projects",
        "create-api-key",
        "projects_createApiKey",
        HttpMethod::Post,
        "/api/public/projects/{projectId}/apiKeys",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("projectId", &self.project_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        let expires_at = f
            .expires_at
            .as_deref()
            .map(|text| {
                chrono::DateTime::parse_from_rfc3339(text)
                    .map(|date| date.with_timezone(&chrono::Utc))
                    .map_err(|error| CliError::usage(format!("--expires-at: {error}")))
            })
            .transpose()?;
        request.body_from(&CreateApiKeyRequest {
            name: f.name.clone(),
            note: f.note.clone(),
            expires_at,
            public_key: f.predefined_public_key.clone(),
            secret_key: f.predefined_secret_key.clone(),
        })
    }
}

path_op! {
    DeleteApiKey {
        /// The project ID
        project_id => "projectId",
        /// The API key ID
        api_key_id => "apiKeyId",
    }
    meta = OperationMeta::new(
        "projects", "delete-api-key", "projects_deleteApiKey",
        HttpMethod::Delete, "/api/public/projects/{projectId}/apiKeys/{apiKeyId}",
    );
}

path_op! {
    ListMemberships {
        /// The project ID
        project_id => "projectId",
    }
    meta = OperationMeta::new(
        "projects", "list-memberships", "organizations_getProjectMemberships",
        HttpMethod::Get, "/api/public/projects/{projectId}/memberships",
    )
    .columns(&["userId", "role", "email", "name"]);
}

#[derive(Debug, Args)]
pub struct UpdateMembership {
    /// The project ID
    project_id: String,
    #[command(flatten)]
    body: MembershipBody,
}

impl Operation for UpdateMembership {
    const META: OperationMeta = OperationMeta::new(
        "projects",
        "update-membership",
        "organizations_updateProjectMembership",
        HttpMethod::Put,
        "/api/public/projects/{projectId}/memberships",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("projectId", &self.project_id);
        self.body.apply(request, ctx)
    }
}

#[derive(Debug, Args)]
pub struct DeleteMembership {
    /// The project ID
    project_id: String,
    #[command(flatten)]
    body: UserBody,
}

impl Operation for DeleteMembership {
    const META: OperationMeta = OperationMeta::new(
        "projects",
        "delete-membership",
        "organizations_deleteProjectMembership",
        HttpMethod::Delete,
        "/api/public/projects/{projectId}/memberships",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("projectId", &self.project_id);
        self.body.apply(request, ctx)
    }
}

resource! {
    pub enum Command {
        /// Get the project of the API key (project-scoped key)
        List(List),
        /// Create a project (organization-scoped key)
        Create(Create),
        /// Update a project (organization-scoped key)
        Update(Update),
        /// Delete a project, asynchronously (organization-scoped key)
        Delete(Delete),
        /// Get the API keys of a project (organization-scoped key)
        ListApiKeys(ListApiKeys),
        /// Create an API key for a project (organization-scoped key)
        CreateApiKey(CreateApiKey),
        /// Delete an API key of a project (organization-scoped key)
        DeleteApiKey(DeleteApiKey),
        /// Get the memberships of a project (organization-scoped key)
        ListMemberships(ListMemberships),
        /// Create or update a project membership; the user must be in the organization
        UpdateMembership(UpdateMembership),
        /// Delete a project membership
        DeleteMembership(DeleteMembership),
    }
}
