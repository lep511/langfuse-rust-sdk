//! `langfuse api health`.

use clap::Args;

use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

#[derive(Debug, Args)]
pub struct Get {}

impl Operation for Get {
    const META: OperationMeta = OperationMeta::new(
        "health",
        "get",
        "health_health",
        HttpMethod::Get,
        "/api/public/health",
    )
    .no_auth();

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Ok(PreparedRequest::new(&Self::META))
    }
}

resource! {
    /// Check the health of the API and its database (no credentials needed)
    pub enum Command {
        /// Check health of API and database
        Get(Get),
    }
}
