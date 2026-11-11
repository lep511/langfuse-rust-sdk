//! `langfuse api llm-connections`, and the organization-level connections of
//! `langfuse api organizations`.

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, bool_value, expand_strings, json_object, tri_state};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

/// The body of both upserts.
#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
pub struct UpsertFields {
    /// Provider name ('openai', 'my-gateway'), unique in the project; the upsert key
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    provider: Option<String>,
    /// The adapter used to call the LLM
    #[arg(
        long,
        value_parser = ["anthropic", "openai", "azure", "bedrock", "google-vertex-ai", "google-ai-studio"],
        required_unless_present_any = ["body_json", "body_file"]
    )]
    adapter: Option<String>,
    /// Secret key of the LLM API (the `secretKey` field)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    provider_secret_key: Option<String>,
    /// Custom base URL of the LLM API (the `baseURL` field)
    #[arg(long)]
    base_url: Option<String>,
    /// Custom model name (repeatable, or one JSON array)
    #[arg(long, value_name = "MODEL")]
    custom_models: Vec<String>,
    /// Include the default models [server default: true]
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_with_default_models"
    )]
    with_default_models: Option<bool>,
    #[arg(long, hide = true, overrides_with = "with_default_models")]
    no_with_default_models: bool,
    /// Extra request headers as a JSON object
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    extra_headers: Option<Value>,
    /// Adapter-specific configuration as a JSON object (Bedrock: {"region": "…"})
    #[arg(long, value_parser = json_object, value_name = "JSON")]
    config: Option<Value>,
}

impl UpsertFields {
    fn body(&self) -> Result<Value> {
        let models = expand_strings("custom-models", &self.custom_models)?;
        Ok(ObjectBody::new()
            .opt("provider", self.provider.clone())
            .opt("adapter", self.adapter.clone())
            .opt("secretKey", self.provider_secret_key.clone())
            .opt("baseURL", self.base_url.clone())
            .items(
                "customModels",
                models.into_iter().map(Value::String).collect(),
            )
            .opt(
                "withDefaultModels",
                tri_state(self.with_default_models, self.no_with_default_models),
            )
            .opt("extraHeaders", self.extra_headers.clone())
            .opt("config", self.config.clone())
            .build())
    }
}

/// An upsert body: field flags or the JSON channel.
#[derive(Debug, Args)]
pub struct UpsertBody {
    #[command(flatten)]
    fields: UpsertFields,
    #[command(flatten)]
    body: BodyChannel,
}

impl UpsertBody {
    fn request(
        &self,
        meta: &'static OperationMeta,
        ctx: &mut Context<'_>,
    ) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(meta);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => Ok(request.body(self.fields.body()?)),
        }
    }
}

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "llm-connections", "list", "llmConnections_list",
        HttpMethod::Get, "/api/public/llm-connections",
    )
    .columns(&["id", "provider", "adapter", "displaySecretKey", "baseURL", "updatedAt"]);
}

#[derive(Debug, Args)]
pub struct Upsert {
    #[command(flatten)]
    inner: UpsertBody,
}

impl Operation for Upsert {
    const META: OperationMeta = OperationMeta::new(
        "llm-connections",
        "upsert",
        "llmConnections_upsert",
        HttpMethod::Put,
        "/api/public/llm-connections",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.inner.request(&Self::META, ctx)
    }
}

path_op! {
    Delete {
        /// The LLM connection ID
        id => "id",
    }
    meta = OperationMeta::new(
        "llm-connections", "delete", "llmConnections_delete",
        HttpMethod::Delete, "/api/public/llm-connections/{id}",
    );
}

page_list_op! {
    ListOrganization {}
    meta = OperationMeta::new(
        "organizations", "list-llm-connections", "llmConnections_listOrganization",
        HttpMethod::Get, "/api/public/organizations/llm-connections",
    )
    .columns(&["id", "provider", "adapter", "displaySecretKey", "baseURL", "updatedAt"]);
}

#[derive(Debug, Args)]
pub struct UpsertOrganization {
    #[command(flatten)]
    inner: UpsertBody,
}

impl Operation for UpsertOrganization {
    const META: OperationMeta = OperationMeta::new(
        "organizations",
        "upsert-llm-connection",
        "llmConnections_upsertOrganization",
        HttpMethod::Put,
        "/api/public/organizations/llm-connections",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.inner.request(&Self::META, ctx)
    }
}

path_op! {
    DeleteOrganization {
        /// The organization-level LLM connection ID
        id => "id",
    }
    meta = OperationMeta::new(
        "organizations", "delete-llm-connection", "llmConnections_deleteOrganization",
        HttpMethod::Delete, "/api/public/organizations/llm-connections/{id}",
    );
}

resource! {
    pub enum Command {
        /// Get the project's LLM connections
        List(List),
        /// Create or update an LLM connection (upserted on --provider)
        Upsert(Upsert),
        /// Delete an LLM connection (evaluators that use it are paused)
        Delete(Delete),
        /// Get the organization's LLM connections (alias of `organizations list-llm-connections`)
        ListLlmConnections(ListOrganization),
        /// Create or update an organization-level LLM connection (alias of `organizations
        /// upsert-llm-connection`)
        UpsertLlmConnection(UpsertOrganization),
        /// Delete an organization-level LLM connection (alias of `organizations
        /// delete-llm-connection`)
        DeleteLlmConnection(DeleteOrganization),
    }
}
