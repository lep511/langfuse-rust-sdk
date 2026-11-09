//! Create, list and delete comments on traces and observations.

use std::sync::Arc;

use serde::Serialize;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::comments::{Comment, CreateCommentRequest, CreateCommentResponse};
use crate::pagination::Paginated;

pub struct CommentsClient {
    pub(crate) http: Arc<HttpClient>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCommentsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    /// Requires `object_type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
}

impl CommentsClient {
    /// `POST /api/public/comments`
    pub async fn create(
        &self,
        req: &CreateCommentRequest,
    ) -> Result<CreateCommentResponse, LangfuseError> {
        self.http.post("/api/public/comments", req).await
    }

    /// `GET /api/public/comments`
    pub async fn list(
        &self,
        params: &ListCommentsParams,
    ) -> Result<Paginated<Comment>, LangfuseError> {
        self.http
            .get_with_query("/api/public/comments", params)
            .await
    }

    /// `GET /api/public/comments/{commentId}`
    pub async fn get(&self, comment_id: &str) -> Result<Comment, LangfuseError> {
        self.http
            .get(&format!(
                "/api/public/comments/{}",
                encode_path_segment(comment_id)
            ))
            .await
    }
}
