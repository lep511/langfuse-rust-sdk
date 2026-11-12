//! `langfuse api annotation-queues`.

use clap::Args;
use langfuse_rust_sdk::api::annotation_queues::ListQueueItemsParams;
use langfuse_rust_sdk::models::annotation_queues::{
    AnnotationQueueAssignmentRequest, AnnotationQueueObjectType, AnnotationQueueStatus,
    CreateAnnotationQueueItemRequest, UpdateAnnotationQueueItemRequest,
};

use crate::args::{AllPages, BodyChannel, PageArgs, required, wire};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

const STATUS: &[&str] = &["PENDING", "COMPLETED"];

page_list_op! {
    List {}
    meta = OperationMeta::new(
        "annotation-queues", "list", "annotationQueues_listQueues",
        HttpMethod::Get, "/api/public/annotation-queues",
    )
    .columns(&["id", "name", "description", "scoreConfigIds", "createdAt"]);
}

json_op! {
    Create {}
    meta = OperationMeta::new(
        "annotation-queues", "create", "annotationQueues_createQueue",
        HttpMethod::Post, "/api/public/annotation-queues",
    );
}

path_op! {
    Get {
        /// The annotation queue ID
        queue_id => "queueId",
    }
    meta = OperationMeta::new(
        "annotation-queues", "get", "annotationQueues_getQueue",
        HttpMethod::Get, "/api/public/annotation-queues/{queueId}",
    );
}

#[derive(Debug, Args)]
pub struct ListItems {
    /// The annotation queue ID
    queue_id: String,
    /// Only items with this status
    #[arg(long, value_parser = wire::<AnnotationQueueStatus>(STATUS), help_heading = "Query")]
    status: Option<AnnotationQueueStatus>,
    #[command(flatten)]
    page: PageArgs,
    #[command(flatten)]
    all: AllPages,
}

impl Operation for ListItems {
    const META: OperationMeta = OperationMeta::new(
        "annotation-queues",
        "list-items",
        "annotationQueues_listQueueItems",
        HttpMethod::Get,
        "/api/public/annotation-queues/{queueId}/items",
    )
    .page()
    .columns(&["id", "objectId", "objectType", "status", "createdAt"]);

    fn prepare(&self, _ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        PreparedRequest::new(&Self::META)
            .path_param("queueId", &self.queue_id)
            .query(&ListQueueItemsParams {
                status: self.status,
                page: self.page.page,
                limit: self.page.limit,
            })
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Some(&self.all)
    }
}

#[derive(Debug, Args)]
pub struct CreateItem {
    /// The annotation queue ID
    queue_id: String,
    #[command(flatten)]
    fields: CreateItemFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct CreateItemFields {
    /// ID of the trace, observation or session to annotate
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    object_id: Option<String>,
    /// Type of the object
    #[arg(
        long,
        value_parser = wire::<AnnotationQueueObjectType>(&["TRACE", "OBSERVATION", "SESSION"]),
        required_unless_present_any = ["body_json", "body_file"]
    )]
    object_type: Option<AnnotationQueueObjectType>,
    /// Status of the new item [server default: PENDING]
    #[arg(long, value_parser = wire::<AnnotationQueueStatus>(STATUS))]
    status: Option<AnnotationQueueStatus>,
}

impl Operation for CreateItem {
    const META: OperationMeta = OperationMeta::new(
        "annotation-queues",
        "create-item",
        "annotationQueues_createQueueItem",
        HttpMethod::Post,
        "/api/public/annotation-queues/{queueId}/items",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("queueId", &self.queue_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let fields = &self.fields;
        request.body_from(&CreateAnnotationQueueItemRequest {
            object_id: fields.object_id.clone().unwrap_or_default(),
            object_type: required("object-type", fields.object_type)?,
            status: fields.status,
        })
    }
}

path_op! {
    GetItem {
        /// The annotation queue ID
        queue_id => "queueId",
        /// The queue item ID
        item_id => "itemId",
    }
    meta = OperationMeta::new(
        "annotation-queues", "get-item", "annotationQueues_getQueueItem",
        HttpMethod::Get, "/api/public/annotation-queues/{queueId}/items/{itemId}",
    );
}

#[derive(Debug, Args)]
pub struct UpdateItem {
    /// The annotation queue ID
    queue_id: String,
    /// The queue item ID
    item_id: String,
    #[command(flatten)]
    fields: UpdateItemFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpdateItemFields {
    /// New status of the item
    #[arg(long, value_parser = wire::<AnnotationQueueStatus>(STATUS))]
    status: Option<AnnotationQueueStatus>,
}

impl Operation for UpdateItem {
    const META: OperationMeta = OperationMeta::new(
        "annotation-queues",
        "update-item",
        "annotationQueues_updateQueueItem",
        HttpMethod::Patch,
        "/api/public/annotation-queues/{queueId}/items/{itemId}",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META)
            .path_param("queueId", &self.queue_id)
            .path_param("itemId", &self.item_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        request.body_from(&UpdateAnnotationQueueItemRequest {
            status: self.fields.status,
        })
    }
}

path_op! {
    DeleteItem {
        /// The annotation queue ID
        queue_id => "queueId",
        /// The queue item ID
        item_id => "itemId",
    }
    meta = OperationMeta::new(
        "annotation-queues", "delete-item", "annotationQueues_deleteQueueItem",
        HttpMethod::Delete, "/api/public/annotation-queues/{queueId}/items/{itemId}",
    );
}

#[derive(Debug, Args)]
pub struct Assignment {
    /// The annotation queue ID
    queue_id: String,
    #[command(flatten)]
    fields: AssignmentFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct AssignmentFields {
    /// The user to assign or unassign
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    user_id: Option<String>,
}

impl Assignment {
    fn request(
        &self,
        meta: &'static OperationMeta,
        ctx: &mut Context<'_>,
    ) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(meta).path_param("queueId", &self.queue_id);
        match self.body.read(ctx)? {
            Some(body) => Ok(request.body(body)),
            None => request.body_from(&AnnotationQueueAssignmentRequest {
                user_id: self.fields.user_id.clone().unwrap_or_default(),
            }),
        }
    }
}

#[derive(Debug, Args)]
pub struct CreateAssignment {
    #[command(flatten)]
    inner: Assignment,
}

impl Operation for CreateAssignment {
    const META: OperationMeta = OperationMeta::new(
        "annotation-queues",
        "create-assignment",
        "annotationQueues_createQueueAssignment",
        HttpMethod::Post,
        "/api/public/annotation-queues/{queueId}/assignments",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.inner.request(&Self::META, ctx)
    }
}

#[derive(Debug, Args)]
pub struct DeleteAssignment {
    #[command(flatten)]
    inner: Assignment,
}

impl Operation for DeleteAssignment {
    const META: OperationMeta = OperationMeta::new(
        "annotation-queues",
        "delete-assignment",
        "annotationQueues_deleteQueueAssignment",
        HttpMethod::Delete,
        "/api/public/annotation-queues/{queueId}/assignments",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        self.inner.request(&Self::META, ctx)
    }
}

resource! {
    pub enum Command {
        /// Get all annotation queues
        List(List),
        /// Create an annotation queue
        #[command(after_help = "Body fields: name (required), scoreConfigIds (required, array), \
            description.\n\nExample:\n  langfuse api annotation-queues create --body-json \
            '{\"name\":\"review\",\"scoreConfigIds\":[\"…\"]}'")]
        Create(Create),
        /// Get an annotation queue by ID
        Get(Get),
        /// Get the items of an annotation queue
        ListItems(ListItems),
        /// Add an item to an annotation queue
        CreateItem(CreateItem),
        /// Get one item of an annotation queue
        GetItem(GetItem),
        /// Update an annotation queue item
        UpdateItem(UpdateItem),
        /// Remove an item from an annotation queue
        DeleteItem(DeleteItem),
        /// Assign a user to an annotation queue
        CreateAssignment(CreateAssignment),
        /// Remove a user's assignment to an annotation queue
        DeleteAssignment(DeleteAssignment),
    }
}
