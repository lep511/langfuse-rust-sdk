//! `langfuse api datasets` (alias `datasets-v2`).

use crate::operation::{HttpMethod, OperationMeta};

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "datasets", "list", "datasets_list",
        HttpMethod::Get, "/api/public/v2/datasets",
    )
    .columns(&["id", "name", "description", "createdAt", "updatedAt"]);
}

json_op! {
    Create {}
    meta = OperationMeta::new(
        "datasets", "create", "datasets_create",
        HttpMethod::Post, "/api/public/v2/datasets",
    );
}

path_op! {
    Get {
        /// The dataset name
        dataset_name => "datasetName",
    }
    meta = OperationMeta::new(
        "datasets", "get", "datasets_get",
        HttpMethod::Get, "/api/public/v2/datasets/{datasetName}",
    );
}

resource! {
    pub enum Command {
        /// Get all datasets
        List(List),
        /// Create a dataset
        #[command(after_help = "Body fields: name (required), description, metadata, inputSchema and \
            expectedOutputSchema (JSON Schemas that validate the items).\n\nExample:\n  \
            langfuse api datasets create --body-json '{\"name\":\"qa\",\"description\":\"Q&A pairs\"}'")]
        Create(Create),
        /// Get a dataset
        Get(Get),
    }
}
