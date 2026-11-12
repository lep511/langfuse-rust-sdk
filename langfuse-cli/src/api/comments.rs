//! `langfuse api comments`.

use clap::Args;
use langfuse_rust_sdk::api::comments::ListCommentsParams;
use langfuse_rust_sdk::models::comments::CreateCommentRequest;

use crate::args::{AllPages, BodyChannel, PageArgs};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

const OBJECT_TYPES: [&str; 4] = ["trace", "observation", "session", "prompt"];

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
    /// Project of the commented object
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    project_id: Option<String>,
    /// Type of the commented object (trace, observation, session, prompt)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    object_type: Option<String>,
    /// ID of the commented object (it must exist)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    object_id: Option<String>,
    /// The comment; markdown allowed, at most 5000 characters
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    content: Option<String>,
    /// Author: a member of the organization that owns the project
    #[arg(long)]
    author_user_id: Option<String>,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "comments",
        "create",
        "comments_create",
        HttpMethod::Post,
        "/api/public/comments",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        let mut comment = CreateCommentRequest::new(
            f.project_id.clone().unwrap_or_default(),
            f.object_type.clone().unwrap_or_default(),
            f.object_id.clone().unwrap_or_default(),
            f.content.clone().unwrap_or_default(),
        );
        comment.author_user_id.clone_from(&f.author_user_id);
        request.body_from(&comment)
    }
}

#[derive(Debug, Args)]
pub struct List {
    /// Only comments on this object type (trace, observation, session, prompt)
    #[arg(long, help_heading = "Query", value_parser = OBJECT_TYPES)]
    object_type: Option<String>,
    /// Only comments on this object (needs --object-type)
    #[arg(long, help_heading = "Query")]
    object_id: Option<String>,
    /// Only comments by this author
    #[arg(long, help_heading = "Query")]
    author_user_id: Option<String>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "comments",
        "list",
        "comments_get",
        HttpMethod::Get,
        "/api/public/comments",
    )
    .page()
    .columns(&[
        "id",
        "objectType",
        "objectId",
        "content",
        "authorUserId",
        "createdAt",
    ]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListCommentsParams {
            page: self.page.page,
            limit: self.page.limit,
            object_type: self.object_type.clone(),
            object_id: self.object_id.clone(),
            author_user_id: self.author_user_id.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

path_op! {
    Get {
        /// The comment ID
        comment_id => "commentId",
    }
    meta = OperationMeta::new(
        "comments", "get", "comments_get-by-id",
        HttpMethod::Get, "/api/public/comments/{commentId}",
    );
}

resource! {
    pub enum Command {
        /// Create a comment on a trace, observation, session or prompt
        Create(Create),
        /// Get all comments
        List(List),
        /// Get a comment by ID
        Get(Get),
    }
}
