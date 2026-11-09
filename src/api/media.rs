//! Upload and manage media assets.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::media::{
    GetMediaResponse, GetMediaUploadUrlRequest, GetMediaUploadUrlResponse, PatchMediaBody,
};

pub struct MediaClient {
    pub(crate) http: Arc<HttpClient>,
}

impl MediaClient {
    /// `GET /api/public/media/{mediaId}`
    pub async fn get(&self, media_id: &str) -> Result<GetMediaResponse, LangfuseError> {
        self.http.get(&media_path(media_id)).await
    }

    /// `PATCH /api/public/media/{mediaId}`: record the outcome of an upload (204).
    pub async fn patch(&self, media_id: &str, body: &PatchMediaBody) -> Result<(), LangfuseError> {
        self.http
            .patch_no_content(&media_path(media_id), body)
            .await
    }

    /// `POST /api/public/media`: get a presigned URL to upload a media file.
    pub async fn get_upload_url(
        &self,
        req: &GetMediaUploadUrlRequest,
    ) -> Result<GetMediaUploadUrlResponse, LangfuseError> {
        self.http.post("/api/public/media", req).await
    }
}

fn media_path(media_id: &str) -> String {
    format!("/api/public/media/{}", encode_path_segment(media_id))
}
