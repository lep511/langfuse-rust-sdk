//! `langfuse api skills` (the raw API; `langfuse skills` installs and publishes).

use clap::Args;
use langfuse_rust_sdk::api::skills::{
    GetSkillParams, ListSkillsParams, SetSkillVersionLabelsRequest, UpdateSkillRequest,
};
use langfuse_rust_sdk::models::skills::{CreateSkillVersionRequest, SkillVersionFileCreateInput};
use serde_json::Value;

use crate::args::{
    AllPages, BodyChannel, PageArgs, bool_value, date_time, expand_objects, expand_strings,
    json_value, tri_state,
};
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct List {
    /// Exact skill name
    #[arg(long, help_heading = "Query")]
    name: Option<String>,
    /// Case-insensitive search in names and latest descriptions
    #[arg(long, help_heading = "Query")]
    search: Option<String>,
    /// Shared skill tag
    #[arg(long, help_heading = "Query")]
    tag: Option<String>,
    /// Skills whose latest version was updated at or after this date-time (RFC 3339)
    #[arg(long, value_parser = date_time, help_heading = "Query")]
    from_updated_at: Option<String>,
    /// Skills whose latest version was updated before this date-time (RFC 3339)
    #[arg(long, value_parser = date_time, help_heading = "Query")]
    to_updated_at: Option<String>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "list",
        "skills_list",
        HttpMethod::Get,
        "/api/public/skills",
    )
    .page()
    .columns(&[
        "name",
        "description",
        "tags",
        "productionVersion",
        "updatedAt",
    ]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListSkillsParams {
            name: self.name.clone(),
            search: self.search.clone(),
            tag: self.tag.clone(),
            page: self.page.page,
            limit: self.page.limit,
            from_updated_at: self.from_updated_at.clone(),
            to_updated_at: self.to_updated_at.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
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
    /// File as {"path", "content"} or {"path", "sha256Hash"} (repeatable, or one JSON array);
    /// the root SKILL.md frontmatter names the skill
    #[arg(long, value_parser = json_value, value_name = "JSON", required_unless_present_any = ["body_json", "body_file"])]
    files: Vec<Value>,
    /// Why this version was published
    #[arg(long)]
    commit_message: Option<String>,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "create",
        "skills_createVersion",
        HttpMethod::Post,
        "/api/public/skills",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let files = expand_objects("files", &self.fields.files)?
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let path = file.get("path").and_then(Value::as_str);
                let content = file.get("content").and_then(Value::as_str);
                let hash = file.get("sha256Hash").and_then(Value::as_str);
                match (path, content, hash) {
                    (Some(path), Some(content), None) => {
                        Ok(SkillVersionFileCreateInput::content(path, content))
                    }
                    (Some(path), None, Some(hash)) => {
                        Ok(SkillVersionFileCreateInput::reference(path, hash))
                    }
                    _ => Err(CliError::usage(format!(
                        "--files: item {index} needs `path` and either `content` or `sha256Hash`"
                    ))),
                }
            })
            .collect::<Result<Vec<_>>>()?;
        request.body_from(&CreateSkillVersionRequest {
            files,
            commit_message: self.fields.commit_message.clone(),
        })
    }
}

#[derive(Debug, Args)]
pub struct Get {
    /// The skill name
    skill_name: String,
    /// Version to resolve
    #[arg(long, value_name = "N", help_heading = "Query")]
    version: Option<i64>,
    /// Label to resolve [server default: production]
    #[arg(long, help_heading = "Query")]
    label: Option<String>,
    /// Include the files' UTF-8 content [server default: false]
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_include_content",
        help_heading = "Query"
    )]
    include_content: Option<bool>,
    #[arg(long, hide = true, overrides_with = "include_content")]
    no_include_content: bool,
}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "get",
        "skills_get",
        HttpMethod::Get,
        "/api/public/skills/{skillName}",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META)
            .path_param("skillName", &self.skill_name)
            .query(&GetSkillParams {
                version: self.version,
                label: self.label.clone(),
                include_content: tri_state(self.include_content, self.no_include_content),
            })
    }
}

#[derive(Debug, Args)]
pub struct Update {
    /// The skill name
    skill_name: String,
    #[command(flatten)]
    fields: TagsFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct TagsFields {
    /// Tag of every version (repeatable, or one JSON array; `[]` clears the tags)
    #[arg(long, value_name = "TAG", required_unless_present_any = ["body_json", "body_file"])]
    tags: Vec<String>,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "update",
        "skills_update",
        HttpMethod::Patch,
        "/api/public/skills/{skillName}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("skillName", &self.skill_name);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&UpdateSkillRequest {
                tags: expand_strings("tags", &self.fields.tags)?,
            }),
        }
    }
}

#[derive(Debug, Args)]
pub struct ListContent {
    /// Canonical base64 SHA-256 hashes from skill manifests, 1 to 50 (comma-separated or
    /// repeatable)
    #[arg(
        long,
        value_delimiter = ',',
        required = true,
        value_name = "HASH",
        help_heading = "Query"
    )]
    sha256_hashes: Vec<String>,
}

impl Operation for ListContent {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "list-content",
        "skills_getFileContents",
        HttpMethod::Get,
        "/api/public/skills/files/content",
    )
    .columns(&["sha256Hash", "content"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META)
            .query_opt("sha256Hashes", Some(self.sha256_hashes.join(","))))
    }
}

#[derive(Debug, Args)]
pub struct UpdateVersion {
    /// The skill name
    skill_name: String,
    /// The skill version
    skill_version: i64,
    #[command(flatten)]
    fields: LabelsFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct LabelsFields {
    /// Label of the version, moved from other versions (repeatable, or one JSON array)
    #[arg(long, value_name = "LABEL", required_unless_present_any = ["body_json", "body_file"])]
    labels: Vec<String>,
}

impl Operation for UpdateVersion {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "update-version",
        "skills_setLabels",
        HttpMethod::Patch,
        "/api/public/skills/{skillName}/versions/{skillVersion}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META)
            .path_param("skillName", &self.skill_name)
            .path_param("skillVersion", self.skill_version);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&SetSkillVersionLabelsRequest {
                labels: expand_strings("labels", &self.fields.labels)?,
            }),
        }
    }
}

#[derive(Debug, Args)]
pub struct DeleteVersion {
    /// The skill name
    skill_name: String,
    /// The skill version
    skill_version: i64,
}

impl Operation for DeleteVersion {
    const META: OperationMeta = OperationMeta::new(
        "skills",
        "delete-version",
        "skills_deleteVersion",
        HttpMethod::Delete,
        "/api/public/skills/{skillName}/versions/{skillVersion}",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META)
            .path_param("skillName", &self.skill_name)
            .path_param("skillVersion", self.skill_version))
    }
}

resource! {
    pub enum Command {
        /// List skills with their latest metadata, shared tags and production version
        List(List),
        /// Create a skill version from its files
        Create(Create),
        /// Resolve a skill's metadata and file manifest by version or label (default: production)
        Get(Get),
        /// Replace the shared tags of every version of a skill
        Update(Update),
        /// Read file contents by their SHA-256 hashes
        ListContent(ListContent),
        /// Replace the labels of a skill version (moving them from other versions)
        UpdateVersion(UpdateVersion),
        /// Delete one immutable skill version
        DeleteVersion(DeleteVersion),
    }
}
