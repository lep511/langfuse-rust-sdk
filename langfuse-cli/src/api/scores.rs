//! `langfuse api scores` (and the `scores-v3` alias resource).

use clap::Args;
use langfuse_rust_sdk::api::scores::ListScoresParams;

use crate::args::{AllPages, CursorArgs, JsonBody, date_time};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct List {
    #[command(flatten)]
    query: ListQuery,
    #[command(flatten)]
    cursor: CursorArgs,
    #[command(flatten)]
    all: AllPages,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct ListQuery {
    /// Comma-separated extra field groups: details, subject, annotation
    #[arg(long)]
    fields: Option<String>,
    /// Comma-separated score IDs
    #[arg(long)]
    id: Option<String>,
    /// Comma-separated score names
    #[arg(long)]
    name: Option<String>,
    /// Comma-separated sources (API, ANNOTATION, EVAL; case-insensitive)
    #[arg(long)]
    source: Option<String>,
    /// Comma-separated data types (NUMERIC, BOOLEAN, CATEGORICAL, TEXT, CORRECTION)
    #[arg(long)]
    data_type: Option<String>,
    /// Comma-separated environments
    #[arg(long)]
    environment: Option<String>,
    /// Comma-separated score config IDs
    #[arg(long)]
    config_id: Option<String>,
    /// Comma-separated annotation queue IDs
    #[arg(long)]
    queue_id: Option<String>,
    /// Comma-separated author user IDs
    #[arg(long)]
    author_user_id: Option<String>,
    /// Comma-separated exact values (needs a single NUMERIC, BOOLEAN or CATEGORICAL --data-type)
    #[arg(long)]
    value: Option<String>,
    /// Inclusive lower bound of the value (needs --data-type NUMERIC)
    #[arg(long, value_name = "NUMBER")]
    value_min: Option<f64>,
    /// Inclusive upper bound of the value (needs --data-type NUMERIC)
    #[arg(long, value_name = "NUMBER")]
    value_max: Option<f64>,
    /// Comma-separated trace IDs
    #[arg(long)]
    trace_id: Option<String>,
    /// Comma-separated session IDs
    #[arg(long)]
    session_id: Option<String>,
    /// Comma-separated observation IDs (needs --trace-id)
    #[arg(long)]
    observation_id: Option<String>,
    /// Comma-separated experiment (dataset run) IDs
    #[arg(long)]
    experiment_id: Option<String>,
    /// Inclusive lower bound of the score timestamp (RFC 3339)
    #[arg(long, value_parser = date_time)]
    from_timestamp: Option<String>,
    /// Exclusive upper bound of the score timestamp (RFC 3339)
    #[arg(long, value_parser = date_time)]
    to_timestamp: Option<String>,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "scores",
        "list",
        "scores_getMany",
        HttpMethod::Get,
        "/api/public/v3/scores",
    )
    .cursor()
    .columns(&["id", "name", "value", "dataType", "source", "timestamp"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let q = &self.query;
        PreparedRequest::new(&Self::META).query(&ListScoresParams {
            limit: self.cursor.limit,
            cursor: self.cursor.cursor.clone(),
            name: q.name.clone(),
            source: q.source.clone(),
            data_type: q.data_type.clone(),
            environment: q.environment.clone(),
            config_id: q.config_id.clone(),
            trace_id: q.trace_id.clone(),
            observation_id: q.observation_id.clone(),
            from_timestamp: q.from_timestamp.clone(),
            to_timestamp: q.to_timestamp.clone(),
            id: q.id.clone(),
            session_id: q.session_id.clone(),
            experiment_id: q.experiment_id.clone(),
            queue_id: q.queue_id.clone(),
            author_user_id: q.author_user_id.clone(),
            value: q.value.clone(),
            value_min: q.value_min,
            value_max: q.value_max,
            fields: q.fields.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct Create {
    #[command(flatten)]
    body: JsonBody,
}

impl Operation for Create {
    const META: OperationMeta = OperationMeta::new(
        "scores",
        "create",
        "scores_create",
        HttpMethod::Post,
        "/api/public/scores",
    )
    .json_only();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META).body(self.body.read(ctx)?))
    }
}

#[derive(Debug, Args)]
pub struct Delete {
    /// The score ID
    score_id: String,
}

impl Operation for Delete {
    const META: OperationMeta = OperationMeta::new(
        "scores",
        "delete",
        "scores_delete",
        HttpMethod::Delete,
        "/api/public/scores/{scoreId}",
    )
    .empty();

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META).path_param("scoreId", &self.score_id))
    }
}

resource! {
    pub enum Command {
        /// Create scores asynchronously from a JSON array of scores
        ///
        /// Returns 202 when every score is accepted, or 207 with accepted/rejected counts and
        /// error messages; do not retry a 207 batch automatically. Each score needs `name` and
        /// `value` (a string for categorical and text scores, a number otherwise).
        #[command(after_help = "Example:\n  langfuse api scores create --body-json \
            '[{\"name\":\"accuracy\",\"value\":0.9,\"traceId\":\"…\",\"dataType\":\"NUMERIC\"}]'")]
        Create(Create),
        /// Delete a score (trace and session scores)
        Delete(Delete),
        /// Get a list of scores with a polymorphic `value` (v3, cursor pagination)
        ///
        /// `value` is a number for NUMERIC, a boolean for BOOLEAN and a string for CATEGORICAL,
        /// TEXT and CORRECTION scores. --fields adds details (comment, configId, metadata),
        /// subject (the scored entity) or annotation (authorUserId, queueId).
        List(List),
    }
}

resource! {
    /// `scores-v3 list`: alias of `scores list`.
    pub enum V3Alias {
        /// Get a list of scores (alias of `scores list`)
        List(List),
    }
}
