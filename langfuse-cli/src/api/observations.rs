//! `langfuse api observations` (alias `observations-v2`).

use clap::Args;
use langfuse_rust_sdk::api::observations::ListObservationsParams;

use crate::args::{AllPages, CursorArgs, bool_value, date_time, json_text, non_empty, tri_state};
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
    /// Comma-separated field groups to return: core, basic, time, io, metadata, model, usage,
    /// prompt, metrics, trace_context
    #[arg(long)]
    fields: Option<String>,
    /// Comma-separated metadata keys to return non-truncated (values over 200 characters are
    /// truncated by default)
    #[arg(long)]
    expand_metadata: Option<String>,
    /// Deprecated: true returns HTTP 400 (input/output are always raw strings)
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_parse_io_as_json"
    )]
    parse_io_as_json: Option<bool>,
    #[arg(long, hide = true, overrides_with = "parse_io_as_json")]
    no_parse_io_as_json: bool,
    /// Filter by observation name
    #[arg(long)]
    name: Option<String>,
    /// Filter by user ID
    #[arg(long)]
    user_id: Option<String>,
    /// Filter by session ID
    #[arg(long)]
    session_id: Option<String>,
    /// Filter by observation type (GENERATION, SPAN, EVENT, AGENT, TOOL, CHAIN, RETRIEVER,
    /// EVALUATOR, EMBEDDING, GUARDRAIL)
    #[arg(long = "type", value_name = "TYPE")]
    kind: Option<String>,
    /// Filter by trace ID
    #[arg(long)]
    trace_id: Option<String>,
    /// Filter by level
    #[arg(long, value_parser = ["DEBUG", "DEFAULT", "WARNING", "ERROR"])]
    level: Option<String>,
    /// Filter by the physical parent observation ID (empty: observations without a parent)
    #[arg(long)]
    parent_observation_id: Option<String>,
    /// Filter by whether an observation is a logical root
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_parser = bool_value,
        overrides_with = "no_is_root_observation"
    )]
    is_root_observation: Option<bool>,
    #[arg(long, hide = true, overrides_with = "is_root_observation")]
    no_is_root_observation: bool,
    /// Only observations in this environment (repeatable)
    #[arg(long, value_name = "ENV")]
    environment: Vec<String>,
    /// Only observations that started at or after this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    from_start_time: Option<String>,
    /// Only observations that started before this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    to_start_time: Option<String>,
    /// Only observations with this version
    #[arg(long)]
    version: Option<String>,
    /// JSON array of filter conditions (see the API reference)
    #[arg(long, value_parser = json_text, value_name = "JSON")]
    filter: Option<String>,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "observations",
        "list",
        "observations_getMany",
        HttpMethod::Get,
        "/api/public/v2/observations",
    )
    .cursor()
    .columns(&["id", "type", "name", "startTime", "traceId", "level"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let q = &self.query;
        PreparedRequest::new(&Self::META).query(&ListObservationsParams {
            limit: self.cursor.limit,
            cursor: self.cursor.cursor.clone(),
            name: q.name.clone(),
            user_id: q.user_id.clone(),
            session_id: q.session_id.clone(),
            r#type: q.kind.clone(),
            trace_id: q.trace_id.clone(),
            level: q.level.clone(),
            parent_observation_id: q.parent_observation_id.clone(),
            environment: non_empty(q.environment.clone()),
            from_start_time: q.from_start_time.clone(),
            to_start_time: q.to_start_time.clone(),
            version: q.version.clone(),
            fields: q.fields.clone(),
            expand_metadata: q.expand_metadata.clone(),
            parse_io_as_json: tri_state(q.parse_io_as_json, q.no_parse_io_as_json),
            is_root_observation: tri_state(q.is_root_observation, q.no_is_root_observation),
            filter: q.filter.clone(),
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

resource! {
    pub enum Command {
        /// Get a list of observations (cursor pagination, field selection)
        ///
        /// Together with OpenTelemetry ingestion and the Metrics API v2, this is the only
        /// real-time read path; other endpoints can lag about 10 minutes. A page may hold fewer
        /// items than --limit (even zero) while meta.cursor is present: continue until it is
        /// absent, or use --all. Without --from-start-time the search starts on 2020-01-01.
        List(List),
    }
}
