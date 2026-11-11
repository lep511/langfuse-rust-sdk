//! `langfuse api scim` (organization-scoped key).

use clap::Args;
use langfuse_rust_sdk::api::scim::ListScimUsersParams;

use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

path_op! {
    GetServiceProviderConfig {}
    meta = OperationMeta::new(
        "scim", "get-service-provider-config", "scim_getServiceProviderConfig",
        HttpMethod::Get, "/api/public/scim/ServiceProviderConfig",
    );
}

path_op! {
    ListResourceTypes {}
    meta = OperationMeta::new(
        "scim", "list-resource-types", "scim_getResourceTypes",
        HttpMethod::Get, "/api/public/scim/ResourceTypes",
    );
}

path_op! {
    ListSchemas {}
    meta = OperationMeta::new(
        "scim", "list-schemas", "scim_getSchemas",
        HttpMethod::Get, "/api/public/scim/Schemas",
    );
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
pub struct ListUsers {
    /// SCIM filter expression, e.g. userName eq "ada@example.com"
    #[arg(long)]
    filter: Option<String>,
    /// 1-based index of the first result [server default: 1]
    #[arg(long, value_name = "N")]
    start_index: Option<i64>,
    /// Maximum results [server default: 100]
    #[arg(long, value_name = "N")]
    count: Option<i64>,
}

impl Operation for ListUsers {
    const META: OperationMeta = OperationMeta::new(
        "scim",
        "list-users",
        "scim_listUsers",
        HttpMethod::Get,
        "/api/public/scim/Users",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListScimUsersParams {
            filter: self.filter.clone(),
            start_index: self.start_index,
            count: self.count,
        })
    }
}

json_op! {
    CreateUser {}
    meta = OperationMeta::new(
        "scim", "create-user", "scim_createUser",
        HttpMethod::Post, "/api/public/scim/Users",
    );
}

path_op! {
    GetUser {
        /// The user ID
        user_id => "userId",
    }
    meta = OperationMeta::new(
        "scim", "get-user", "scim_getUser",
        HttpMethod::Get, "/api/public/scim/Users/{userId}",
    );
}

path_op! {
    DeleteUser {
        /// The user ID
        user_id => "userId",
    }
    meta = OperationMeta::new(
        "scim", "delete-user", "scim_deleteUser",
        HttpMethod::Delete, "/api/public/scim/Users/{userId}",
    );
}

resource! {
    pub enum Command {
        /// Get the SCIM service provider configuration
        GetServiceProviderConfig(GetServiceProviderConfig),
        /// Get the SCIM resource types
        ListResourceTypes(ListResourceTypes),
        /// Get the SCIM schemas
        ListSchemas(ListSchemas),
        /// List the organization's users
        ListUsers(ListUsers),
        /// Create a user in the organization
        #[command(after_help = "Body fields: userName (the email, required), name ({givenName, \
            familyName, formatted}, required), emails, active, password (ignored).\n\nExample:\n  \
            langfuse api scim create-user --body-json \
            '{\"userName\":\"ada@example.com\",\"name\":{\"formatted\":\"Ada\"}}'")]
        CreateUser(CreateUser),
        /// Get a user
        GetUser(GetUser),
        /// Remove a user from the organization (the user entity is kept)
        DeleteUser(DeleteUser),
    }
}
