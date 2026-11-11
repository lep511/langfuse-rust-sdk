//! `langfuse api traces` (alias `trace`).

use clap::Args;
use langfuse_rust_sdk::models::traces::DeleteTracesRequest;

use crate::args::{BodyChannel, expand_strings};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

path_op! {
    Delete {
        /// The trace ID
        trace_id => "traceId",
    }
    meta = OperationMeta::new(
        "traces", "delete", "trace_delete",
        HttpMethod::Delete, "/api/public/traces/{traceId}",
    );
}

#[derive(Debug, Args)]
pub struct DeleteMany {
    #[command(flatten)]
    fields: DeleteManyFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct DeleteManyFields {
    /// Trace to delete (repeatable, or one JSON array)
    #[arg(
        long = "trace-ids",
        alias = "trace-id",
        value_name = "TRACE_ID",
        required_unless_present_any = ["body_json", "body_file"]
    )]
    trace_ids: Vec<String>,
}

impl Operation for DeleteMany {
    const META: OperationMeta = OperationMeta::new(
        "traces",
        "delete-many",
        "trace_deleteMultiple",
        HttpMethod::Delete,
        "/api/public/traces",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&DeleteTracesRequest {
                trace_ids: expand_strings("trace-ids", &self.fields.trace_ids)?,
            }),
        }
    }
}

resource! {
    pub enum Command {
        /// Delete a trace
        Delete(Delete),
        /// Delete several traces
        DeleteMany(DeleteMany),
    }
}
