//! Annotation queue and queue-item management.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::annotation_queues::{
    AnnotationQueue, AnnotationQueueAssignmentRequest, AnnotationQueueItem, AnnotationQueueStatus,
    CreateAnnotationQueueAssignmentResponse, CreateAnnotationQueueItemRequest,
    CreateAnnotationQueueRequest, DeleteAnnotationQueueAssignmentResponse,
    DeleteAnnotationQueueItemResponse, UpdateAnnotationQueueItemRequest,
};
use crate::pagination::{PageParams, Paginated};

const BASE: &str = "/api/public/annotation-queues";

pub struct AnnotationQueuesClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQueueItemsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<AnnotationQueueStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

impl AnnotationQueuesClient {
    /// `GET /api/public/annotation-queues`
    pub async fn list_queues(
        &self,
        params: &PageParams,
    ) -> Result<Paginated<AnnotationQueue>, LangfuseError> {
        self.http.get_with_query(BASE, params).await
    }

    /// `POST /api/public/annotation-queues`
    pub async fn create_queue(
        &self,
        req: &CreateAnnotationQueueRequest,
    ) -> Result<AnnotationQueue, LangfuseError> {
        self.http.post(BASE, req).await
    }

    /// `GET /api/public/annotation-queues/{queueId}`
    pub async fn get_queue(&self, queue_id: &str) -> Result<AnnotationQueue, LangfuseError> {
        self.http.get(&queue_path(queue_id)).await
    }

    /// `GET /api/public/annotation-queues/{queueId}/items`
    pub async fn list_items(
        &self,
        queue_id: &str,
        params: &ListQueueItemsParams,
    ) -> Result<Paginated<AnnotationQueueItem>, LangfuseError> {
        self.http
            .get_with_query(&format!("{}/items", queue_path(queue_id)), params)
            .await
    }

    /// `POST /api/public/annotation-queues/{queueId}/items`
    pub async fn create_item(
        &self,
        queue_id: &str,
        req: &CreateAnnotationQueueItemRequest,
    ) -> Result<AnnotationQueueItem, LangfuseError> {
        self.http
            .post(&format!("{}/items", queue_path(queue_id)), req)
            .await
    }

    /// `GET /api/public/annotation-queues/{queueId}/items/{itemId}`
    pub async fn get_item(
        &self,
        queue_id: &str,
        item_id: &str,
    ) -> Result<AnnotationQueueItem, LangfuseError> {
        self.http.get(&item_path(queue_id, item_id)).await
    }

    /// `PATCH /api/public/annotation-queues/{queueId}/items/{itemId}`
    pub async fn update_item(
        &self,
        queue_id: &str,
        item_id: &str,
        req: &UpdateAnnotationQueueItemRequest,
    ) -> Result<AnnotationQueueItem, LangfuseError> {
        self.http.patch(&item_path(queue_id, item_id), req).await
    }

    /// `DELETE /api/public/annotation-queues/{queueId}/items/{itemId}`
    pub async fn delete_item(
        &self,
        queue_id: &str,
        item_id: &str,
    ) -> Result<DeleteAnnotationQueueItemResponse, LangfuseError> {
        self.http
            .delete_with_response(&item_path(queue_id, item_id))
            .await
    }

    /// `POST /api/public/annotation-queues/{queueId}/assignments`
    pub async fn create_assignment(
        &self,
        queue_id: &str,
        user_id: &str,
    ) -> Result<CreateAnnotationQueueAssignmentResponse, LangfuseError> {
        let body = AnnotationQueueAssignmentRequest {
            user_id: user_id.to_string(),
        };
        self.http
            .post(&format!("{}/assignments", queue_path(queue_id)), &body)
            .await
    }

    /// `DELETE /api/public/annotation-queues/{queueId}/assignments` with `{userId}`.
    pub async fn delete_assignment(
        &self,
        queue_id: &str,
        user_id: &str,
    ) -> Result<DeleteAnnotationQueueAssignmentResponse, LangfuseError> {
        let body = AnnotationQueueAssignmentRequest {
            user_id: user_id.to_string(),
        };
        self.http
            .delete_with_body_response(&format!("{}/assignments", queue_path(queue_id)), &body)
            .await
    }
}

fn queue_path(queue_id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(queue_id))
}

fn item_path(queue_id: &str, item_id: &str) -> String {
    format!(
        "{}/items/{}",
        queue_path(queue_id),
        encode_path_segment(item_id)
    )
}
