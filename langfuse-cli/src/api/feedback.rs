//! `langfuse api feedback`.

use clap::Args;
use langfuse_rust_sdk::models::feedback::{FeedbackTargetType, SubmitFeedbackRequest};

use crate::args::BodyChannel;
use crate::error::{CliError, Result};
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct Submit {
    #[command(flatten)]
    fields: SubmitFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct SubmitFields {
    /// Category of the thing the feedback is about
    #[arg(
        long,
        value_parser = ["skill", "mcp-tool", "cli", "docs", "public-api", "other"],
        required_unless_present_any = ["body_json", "body_file"]
    )]
    target_type: Option<String>,
    /// The instance: skill name, MCP tool, CLI command, API endpoint path or docs page
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    target: Option<String>,
    /// Feedback text approved by the user (1–3000 characters)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    feedback: Option<String>,
    /// The goal or use case (1–1500 characters)
    #[arg(long)]
    goal: Option<String>,
    /// HTTP(S) reference URL (stored as text, not fetched)
    #[arg(long)]
    reference_url: Option<String>,
}

fn target_type(value: &str) -> Result<FeedbackTargetType> {
    Ok(match value {
        "skill" => FeedbackTargetType::Skill,
        "mcp-tool" => FeedbackTargetType::McpTool,
        "cli" => FeedbackTargetType::Cli,
        "docs" => FeedbackTargetType::Docs,
        "public-api" => FeedbackTargetType::PublicApi,
        "other" => FeedbackTargetType::Other,
        other => {
            return Err(CliError::usage(format!(
                "--target-type: unsupported value {other}"
            )));
        }
    })
}

impl Operation for Submit {
    const META: OperationMeta = OperationMeta::new(
        "feedback",
        "submit",
        "feedback_submit",
        HttpMethod::Post,
        "/api/public/feedback",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        request.body_from(&SubmitFeedbackRequest {
            target_type: target_type(f.target_type.as_deref().unwrap_or_default())?,
            target: f.target.clone().unwrap_or_default(),
            feedback: f.feedback.clone().unwrap_or_default(),
            goal: f.goal.clone(),
            reference_url: f.reference_url.clone(),
        })
    }
}

resource! {
    pub enum Command {
        /// Submit user-approved feedback about Langfuse skills, MCP tools, CLI, docs or API
        ///
        /// Do not include secrets, credentials, customer data or trace payloads. Not available in
        /// the HIPAA cloud region.
        Submit(Submit),
    }
}
