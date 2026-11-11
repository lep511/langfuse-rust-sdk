//! `langfuse api dataset-items`.

use clap::Args;
use langfuse_rust_sdk::api::dataset_items::ListDatasetItemsParams;

use crate::args::{AllPages, PageArgs, date_time};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

json_op! {
    Create {}
    meta = OperationMeta::new(
        "dataset-items", "create", "datasetItems_create",
        HttpMethod::Post, "/api/public/dataset-items",
    );
}

#[derive(Debug, Args)]
pub struct List {
    /// Only items of this dataset
    #[arg(long, help_heading = "Query")]
    dataset_name: Option<String>,
    /// Only items created from this trace
    #[arg(long, help_heading = "Query")]
    source_trace_id: Option<String>,
    /// Only items created from this observation
    #[arg(long, help_heading = "Query")]
    source_observation_id: Option<String>,
    /// The dataset as it was at this date-time (RFC 3339, UTC); needs --dataset-name
    #[arg(long, value_parser = date_time, help_heading = "Query")]
    version: Option<String>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for List {
    const META: OperationMeta = OperationMeta::new(
        "dataset-items",
        "list",
        "datasetItems_list",
        HttpMethod::Get,
        "/api/public/dataset-items",
    )
    .page()
    .columns(&["id", "datasetName", "status", "sourceTraceId", "createdAt"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META).query(&ListDatasetItemsParams {
            dataset_name: self.dataset_name.clone(),
            source_trace_id: self.source_trace_id.clone(),
            source_observation_id: self.source_observation_id.clone(),
            version: self.version.clone(),
            page: self.page.page,
            limit: self.page.limit,
        })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

path_op! {
    Get {
        /// The dataset item ID
        id => "id",
    }
    meta = OperationMeta::new(
        "dataset-items", "get", "datasetItems_get",
        HttpMethod::Get, "/api/public/dataset-items/{id}",
    );
}

path_op! {
    Delete {
        /// The dataset item ID
        id => "id",
    }
    meta = OperationMeta::new(
        "dataset-items", "delete", "datasetItems_delete",
        HttpMethod::Delete, "/api/public/dataset-items/{id}",
    );
}

resource! {
    pub enum Command {
        /// Create (upsert on `id`) a dataset item
        #[command(after_help = "Body fields: datasetName (required), input, expectedOutput, metadata, \
            sourceTraceId, sourceObservationId, id, status (ACTIVE|ARCHIVED).\n\nExample:\n  \
            langfuse api dataset-items create --body-json \
            '{\"datasetName\":\"qa\",\"input\":{\"q\":\"2+2\"},\"expectedOutput\":\"4\"}'")]
        Create(Create),
        /// Get dataset items, optionally as they were at --version
        List(List),
        /// Get a dataset item
        Get(Get),
        /// Delete a dataset item and all its run items (irreversible)
        Delete(Delete),
    }
}
