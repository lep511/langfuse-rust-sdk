//! `langfuse api experiments` and `langfuse api experiment-items`.

use clap::Args;
use langfuse_rust_sdk::api::experiments::{ListExperimentItemsParams, ListExperimentsParams};

use crate::args::{AllPages, CursorArgs, date_time, json_text};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

/// Query flags shared by both lists.
#[derive(Debug, Args)]
#[command(next_help_heading = "Query")]
struct Window {
    /// Required lower bound: only data on or after this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    from_start_time: String,
    /// Only data before this date-time (RFC 3339)
    #[arg(long, value_parser = date_time)]
    to_start_time: Option<String>,
    /// Scores per result with --fields scores (at most 50)
    #[arg(long, value_name = "N")]
    score_limit: Option<i64>,
    /// Comma-separated dataset IDs
    #[arg(long)]
    dataset_id: Option<String>,
    /// JSON array of structured filter conditions
    #[arg(long, value_parser = json_text, value_name = "JSON")]
    filter: Option<String>,
}

#[derive(Debug, Args)]
pub struct List {
    /// Comma-separated field groups: core, metadata, scores [default: core]
    #[arg(long, help_heading = "Query")]
    fields: Option<String>,
    /// Comma-separated experiment IDs
    #[arg(long, help_heading = "Query")]
    id: Option<String>,
    /// Comma-separated experiment names
    #[arg(long, help_heading = "Query")]
    name: Option<String>,
    #[command(flatten)]
    window: Window,
    #[command(flatten)]
    cursor: CursorArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "experiments",
        "list",
        "experiments_list",
        HttpMethod::Get,
        "/api/public/experiments",
    )
    .cursor()
    .columns(&["id", "name", "datasetId", "startTime"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let w = &self.window;
        let mut params = ListExperimentsParams::new(w.from_start_time.clone());
        params.fields.clone_from(&self.fields);
        params.limit = self.cursor.limit;
        params.score_limit = w.score_limit;
        params.cursor.clone_from(&self.cursor.cursor);
        params.to_start_time.clone_from(&w.to_start_time);
        params.id.clone_from(&self.id);
        params.name.clone_from(&self.name);
        params.dataset_id.clone_from(&w.dataset_id);
        params.filter.clone_from(&w.filter);
        PreparedRequest::new(&Self::META).query(&params)
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct ListItems {
    /// Comma-separated field groups: core, dataset, io, metadata, itemMetadata,
    /// experimentMetadata, scores [default: core]
    #[arg(long, help_heading = "Query")]
    fields: Option<String>,
    /// Comma-separated experiment IDs
    #[arg(long, help_heading = "Query")]
    experiment_id: Option<String>,
    /// Comma-separated experiment names
    #[arg(long, help_heading = "Query")]
    experiment_name: Option<String>,
    /// Comma-separated experiment item IDs
    #[arg(long, help_heading = "Query")]
    experiment_item_id: Option<String>,
    #[command(flatten)]
    window: Window,
    #[command(flatten)]
    cursor: CursorArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListItems {
    const META: OperationMeta = OperationMeta::new(
        "experiment-items",
        "list",
        "experiments_listItems",
        HttpMethod::Get,
        "/api/public/experiment-items",
    )
    .cursor()
    .columns(&["id", "experimentId", "experimentName", "startTime"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let w = &self.window;
        let mut params = ListExperimentItemsParams::new(w.from_start_time.clone());
        params.fields.clone_from(&self.fields);
        params.limit = self.cursor.limit;
        params.score_limit = w.score_limit;
        params.cursor.clone_from(&self.cursor.cursor);
        params.to_start_time.clone_from(&w.to_start_time);
        params.experiment_id.clone_from(&self.experiment_id);
        params.experiment_name.clone_from(&self.experiment_name);
        params
            .experiment_item_id
            .clone_from(&self.experiment_item_id);
        params.dataset_id.clone_from(&w.dataset_id);
        params.filter.clone_from(&w.filter);
        PreparedRequest::new(&Self::META).query(&params)
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

resource! {
    pub enum Command {
        /// List experiments, latest activity first (cursor pagination; --from-start-time required)
        List(List),
    }
}

resource! {
    pub enum ItemsCommand {
        /// List experiment items (inputs, outputs, expected outputs, metadata, scores), newest
        /// first (cursor pagination; --from-start-time required)
        List(ListItems),
    }
}
