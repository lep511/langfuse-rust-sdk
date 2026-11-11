//! `langfuse api metrics` (alias `metrics-v2`).

use clap::Args;

use crate::args::json_text;
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct Get {
    /// The query as JSON: {"view", "metrics", "dimensions", "filters", "fromTimestamp",
    /// "toTimestamp", "timeDimension", "orderBy", "config"}
    #[arg(long, value_parser = json_text, value_name = "JSON", help_heading = "Query")]
    query: String,
}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "metrics",
        "get",
        "metrics_metrics",
        HttpMethod::Get,
        "/api/public/v2/metrics",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META).query_opt("query", Some(&self.query)))
    }
}

resource! {
    pub enum Command {
        /// Query metrics of the project (v2, real-time)
        ///
        /// Together with OpenTelemetry ingestion and the Observations API v2, this is the only
        /// real-time read path.
        #[command(after_help = "Example:\n  langfuse api metrics get --query \
            '{\"view\":\"observations\",\"metrics\":[{\"measure\":\"count\",\"aggregation\":\"count\"}],\
            \"fromTimestamp\":\"2026-11-01T00:00:00Z\",\"toTimestamp\":\"2026-11-02T00:00:00Z\"}'")]
        Get(Get),
    }
}
