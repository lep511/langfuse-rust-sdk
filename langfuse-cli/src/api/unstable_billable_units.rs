//! `langfuse api unstable-billable-units` (feature `unstable`).

use clap::Args;

use crate::args::date_time;
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct Get {
    /// Start of the range (inclusive), ISO 8601 with a time zone
    #[arg(long, value_parser = date_time, help_heading = "Query")]
    from_timestamp: String,
    /// End of the range (exclusive), ISO 8601 with a time zone
    #[arg(long, value_parser = date_time, help_heading = "Query")]
    to_timestamp: String,
    /// Bucket size: hour, day, week, month [default: day]
    #[arg(long, value_parser = ["hour", "day", "week", "month"], help_heading = "Query")]
    granularity: Option<String>,
}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "unstable-billable-units",
        "get",
        "unstable_billableUnits_get",
        HttpMethod::Get,
        "/api/public/unstable/organizations/billable-units",
    );

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META)
            .query_opt("fromTimestamp", Some(&self.from_timestamp))
            .query_opt("toTimestamp", Some(&self.to_timestamp))
            .query_opt("granularity", self.granularity.as_ref()))
    }
}

resource! {
    pub enum Command {
        /// Query billable units per UTC time bucket and project (organization-scoped key)
        Get(Get),
    }
}
