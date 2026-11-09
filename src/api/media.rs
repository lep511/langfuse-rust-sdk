//! Upload and manage media assets.
//!
//! For uploads use [`LangfuseMedia::upload`](crate::media::LangfuseMedia::upload),
//! which wraps the three calls of the upload protocol.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::media::{DOWNLOAD_TIMEOUT, MAX_DEPTH, MediaReference, find_references};
use crate::models::media::{
    GetMediaResponse, GetMediaUploadUrlRequest, GetMediaUploadUrlResponse, PatchMediaBody,
};

/// Media metadata and content, from [`MediaClient::download`].
#[derive(Debug, Clone)]
pub struct DownloadedMedia {
    /// Metadata, including the content type.
    pub media: GetMediaResponse,
    /// The content.
    pub bytes: Vec<u8>,
}

impl DownloadedMedia {
    /// The content as a base64 data URI.
    pub fn to_data_uri(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.media.content_type,
            STANDARD.encode(&self.bytes)
        )
    }
}

#[derive(Clone)]
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

    /// Fetch the metadata of media `media_id` and download its content from the
    /// signed URL (without Langfuse credentials).
    pub async fn download(&self, media_id: &str) -> Result<DownloadedMedia, LangfuseError> {
        let media = self.get(media_id).await?;
        let response = self
            .http
            .raw_client()
            .get(&media.url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(LangfuseError::Media(format!(
                "download of media {media_id} failed with HTTP {status}"
            )));
        }
        let bytes = response.bytes().await?.to_vec();
        Ok(DownloadedMedia { media, bytes })
    }

    /// A copy of `value` with every `@@@langfuseMedia:...@@@` reference (in
    /// strings up to 10 levels deep) replaced by the media as a base64 data
    /// URI. References that cannot be resolved are left unchanged and logged.
    /// Each distinct media is downloaded once.
    pub async fn resolve_references(&self, value: &Value) -> Value {
        let mut references = BTreeSet::new();
        collect_references(value, 0, &mut references);
        let mut resolved: HashMap<String, String> = HashMap::new();
        for reference in references {
            let outcome = match MediaReference::parse(&reference) {
                Ok(parsed) => self
                    .download(&parsed.media_id)
                    .await
                    .map(|m| m.to_data_uri()),
                Err(err) => Err(err),
            };
            match outcome {
                Ok(data_uri) => {
                    resolved.insert(reference, data_uri);
                }
                Err(err) => {
                    tracing::warn!(reference = %reference, error = %err, "could not resolve media reference")
                }
            }
        }
        replace_references(value, 0, &resolved)
    }
}

fn collect_references(value: &Value, depth: usize, out: &mut BTreeSet<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Value::String(text) => out.extend(find_references(text).into_iter().map(str::to_owned)),
        Value::Array(items) => items
            .iter()
            .for_each(|v| collect_references(v, depth + 1, out)),
        Value::Object(map) => map
            .values()
            .for_each(|v| collect_references(v, depth + 1, out)),
        _ => {}
    }
}

fn replace_references(value: &Value, depth: usize, resolved: &HashMap<String, String>) -> Value {
    if depth > MAX_DEPTH {
        return value.clone();
    }
    match value {
        Value::String(text) => {
            let mut out = text.clone();
            for reference in find_references(text) {
                if let Some(data_uri) = resolved.get(reference) {
                    out = out.replace(reference, data_uri);
                }
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| replace_references(v, depth + 1, resolved))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), replace_references(v, depth + 1, resolved)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn media_path(media_id: &str) -> String {
    format!("/api/public/media/{}", encode_path_segment(media_id))
}
