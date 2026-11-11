//! `langfuse api prompts` (and the `prompt-version` alias resource).

use clap::{Args, ValueEnum};
use langfuse_rust_sdk::api::prompt_version::UpdatePromptVersionRequest;
use langfuse_rust_sdk::api::prompts::{DeletePromptParams, GetPromptParams, ListPromptsParams};
use langfuse_rust_sdk::models::prompts::{ChatPromptMessage, CreatePromptRequest};
use serde_json::Value;

use crate::args::{
    AllPages, BodyChannel, PageArgs, bool_value, date_time, expand_items, expand_strings,
    json_text, json_value, non_empty, tri_state,
};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct List {
    #[command(flatten)]
    query: ListQuery,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct ListQuery {
    /// Only prompts with this name
    #[arg(long)]
    name: Option<String>,
    /// Only prompts with a version that has this label
    #[arg(long)]
    label: Option<String>,
    /// Only prompts with this tag
    #[arg(long)]
    tag: Option<String>,
    /// Only prompt versions created or updated at or after this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    from_updated_at: Option<String>,
    /// Only prompt versions created or updated before this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    to_updated_at: Option<String>,
    /// JSON array of filter conditions, combined with AND; takes precedence over the other filters
    #[arg(long, value_parser = json_text, value_name = "JSON")]
    filter: Option<String>,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "list",
        "prompts_list",
        HttpMethod::Get,
        "/api/public/v2/prompts",
    )
    .page()
    .columns(&[
        "name",
        "type",
        "versions",
        "labels",
        "tags",
        "lastUpdatedAt",
    ]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let q = &self.query;
        PreparedRequest::new(&Self::META).query(&ListPromptsParams {
            name: q.name.clone(),
            label: q.label.clone(),
            tag: q.tag.clone(),
            page: self.page.page,
            limit: self.page.limit,
            from_updated_at: q.from_updated_at.clone(),
            to_updated_at: q.to_updated_at.clone(),
            filter: q.filter.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct Get {
    /// The prompt name, folder path included (folder/name)
    prompt_name: String,
    #[command(flatten)]
    query: GetQuery,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct GetQuery {
    /// Version to retrieve
    #[arg(long, alias = "prompt-version", value_name = "N")]
    version: Option<i64>,
    /// Label to retrieve (default: production when neither label nor version is set)
    #[arg(long)]
    label: Option<String>,
    /// Resolve prompt dependencies (server default: true); --no-resolve returns the raw prompt
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_resolve"
    )]
    resolve: Option<bool>,
    #[arg(long, hide = true, overrides_with = "resolve")]
    no_resolve: bool,
}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "get",
        "prompts_get",
        HttpMethod::Get,
        "/api/public/v2/prompts/{promptName}",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let q = &self.query;
        PreparedRequest::new(&Self::META)
            .path_param("promptName", &self.prompt_name)
            .query(&GetPromptParams {
                version: q.version,
                label: q.label.clone(),
                resolve: tri_state(q.resolve, q.no_resolve),
            })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PromptType {
    Chat,
    Text,
}

#[derive(Debug, Args)]
pub struct Create {
    #[command(flatten)]
    fields: CreateFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct CreateFields {
    /// Prompt type; selects the body variant (also sent as `type`)
    #[arg(long = "type", value_name = "TYPE", value_enum, required_unless_present_any = ["body_json", "body_file"])]
    kind: Option<PromptType>,
    /// The full prompt name, folder path included (at most 255 characters)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    name: Option<String>,
    /// The prompt: one string (--type text), or chat messages as JSON objects (--type chat;
    /// repeatable, or one JSON array)
    #[arg(long, value_name = "PROMPT", required_unless_present_any = ["body_json", "body_file"])]
    prompt: Vec<String>,
    /// Model config of this version (any JSON)
    #[arg(long, value_parser = json_value, value_name = "JSON")]
    config: Option<Value>,
    /// Deployment label of this version (repeatable, or one JSON array)
    #[arg(long = "labels", value_name = "LABEL")]
    labels: Vec<String>,
    /// Tag of all versions of this prompt (repeatable, or one JSON array)
    #[arg(long = "tags", value_name = "TAG")]
    tags: Vec<String>,
    /// Commit message of this version
    #[arg(long)]
    commit_message: Option<String>,
}

impl CreateFields {
    fn request(&self) -> Result<CreatePromptRequest> {
        let name = self.name.clone().unwrap_or_default();
        let labels = non_empty(expand_strings("labels", &self.labels)?);
        let tags = non_empty(expand_strings("tags", &self.tags)?);
        let config = self.config.clone();
        let commit_message = self.commit_message.clone();
        match self.kind {
            Some(PromptType::Text) => {
                let [prompt] = self.prompt.as_slice() else {
                    return Err(CliError::usage(
                        "--prompt: a text prompt (--type text) takes exactly one value",
                    ));
                };
                Ok(CreatePromptRequest::Text {
                    name,
                    prompt: prompt.clone(),
                    config,
                    labels,
                    tags,
                    commit_message,
                })
            }
            Some(PromptType::Chat) => {
                let values = self
                    .prompt
                    .iter()
                    .map(|text| {
                        json_value(text).map_err(|_| {
                            CliError::usage(format!(
                                "--prompt: expected a chat message as a JSON object, got {text}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let prompt = expand_items(&values)
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        if !item.is_object() {
                            return Err(CliError::usage(format!(
                                "--prompt: array item {index} must be an object"
                            )));
                        }
                        serde_json::from_value::<ChatPromptMessage>(item).map_err(|error| {
                            CliError::usage(format!(
                                "--prompt: item {index} is not a chat message: {error}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(CreatePromptRequest::Chat {
                    name,
                    prompt,
                    config,
                    labels,
                    tags,
                    commit_message,
                })
            }
            None => Err(CliError::usage(
                "--type is required to build the body from field flags",
            )),
        }
    }
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "create",
        "prompts_create",
        HttpMethod::Post,
        "/api/public/v2/prompts",
    )
    .union("type");

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&self.fields.request()?),
        }
    }
}

#[derive(Debug, Args)]
pub struct Delete {
    /// The prompt name, folder path included (folder/name)
    prompt_name: String,
    /// Delete only the versions with this label
    #[arg(long, help_heading = "Query")]
    label: Option<String>,
    /// Delete only this version
    #[arg(long, value_name = "N", help_heading = "Query")]
    version: Option<i64>,
}

impl Operation for Delete {
    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "delete",
        "prompts_delete",
        HttpMethod::Delete,
        "/api/public/v2/prompts/{promptName}",
    )
    .empty();

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META)
            .path_param("promptName", &self.prompt_name)
            .query(&DeletePromptParams {
                label: self.label.clone(),
                version: self.version,
            })
    }
}

#[derive(Debug, Args)]
pub struct UpdateVersion {
    /// The prompt name, folder path included (folder/name)
    name: String,
    /// The version to update
    version: i64,
    #[command(flatten)]
    fields: UpdateVersionFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpdateVersionFields {
    /// New label of the version (repeatable, or one JSON array); `latest` is reserved
    #[arg(
        long = "new-labels",
        alias = "new-label",
        value_name = "LABEL",
        required_unless_present_any = ["body_json", "body_file"]
    )]
    new_labels: Vec<String>,
}

impl Operation for UpdateVersion {
    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "update-version",
        "promptVersion_update",
        HttpMethod::Patch,
        "/api/public/v2/prompts/{name}/versions/{version}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META)
            .path_param("name", &self.name)
            .path_param("version", self.version);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&UpdatePromptVersionRequest {
                new_labels: expand_strings("new-labels", &self.fields.new_labels)?,
            }),
        }
    }
}

resource! {
    pub enum Command {
        /// Get a list of prompt names with versions and labels
        List(List),
        /// Get a prompt (by default its production version)
        Get(Get),
        /// Create a new version of the prompt with the given name
        ///
        /// Field flags build the body: --type selects the variant (chat or text) and its
        /// required fields. --body-json / --body-file send any body unchanged.
        Create(Create),
        /// Delete prompt versions; without --label or --version, every version is deleted
        Delete(Delete),
        /// Update the labels of one prompt version
        UpdateVersion(UpdateVersion),
    }
}

resource! {
    /// `prompt-version update-version`: alias of `prompts update-version`.
    pub enum VersionAlias {
        /// Update the labels of one prompt version (alias of `prompts update-version`)
        UpdateVersion(UpdateVersion),
    }
}
