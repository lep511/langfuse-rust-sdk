use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Response of `GET /api/public/media/{mediaId}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaResponse {
    pub media_id: String,
    pub content_type: String,
    pub content_length: i64,
    pub uploaded_at: DateTime<Utc>,
    pub url: String,
    /// Expiry of the download URL, as returned by the server (unformatted string in the spec).
    pub url_expiry: String,
}

/// Body of `PATCH /api/public/media/{mediaId}`: report the result of an upload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchMediaBody {
    pub uploaded_at: DateTime<Utc>,
    pub upload_http_status: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_http_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_time_ms: Option<i64>,
}

/// Body of `POST /api/public/media`: request a presigned upload URL.
///
/// Provide exactly one context: a trace (`trace_id`, optionally `observation_id`)
/// or a dataset item (`dataset_id` + `dataset_item_id`). `field` must match the
/// context: `input`/`output`/`metadata` for traces, `input`/`expectedOutput`/`metadata`
/// for dataset items.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaUploadUrlRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_item_id: Option<String>,
    pub content_type: MediaContentType,
    pub content_length: i64,
    pub sha256_hash: String,
    pub field: String,
}

impl GetMediaUploadUrlRequest {
    /// A request with the required fields set and no context; set one context afterwards.
    pub fn new(
        content_type: MediaContentType,
        content_length: i64,
        sha256_hash: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self {
            trace_id: None,
            observation_id: None,
            dataset_id: None,
            dataset_item_id: None,
            content_type,
            content_length,
            sha256_hash: sha256_hash.into(),
            field: field.into(),
        }
    }
}

/// Response of `POST /api/public/media`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaUploadUrlResponse {
    /// Presigned upload URL; `None` if the asset is already uploaded.
    pub upload_url: Option<String>,
    pub media_id: String,
}

/// MIME type of a media upload (spec `MediaContentType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MediaContentType {
    #[serde(rename = "image/png")]
    ImagePng,
    #[serde(rename = "image/jpeg")]
    ImageJpeg,
    #[serde(rename = "image/jpg")]
    ImageJpg,
    #[serde(rename = "image/webp")]
    ImageWebp,
    #[serde(rename = "image/gif")]
    ImageGif,
    #[serde(rename = "image/svg+xml")]
    ImageSvgXml,
    #[serde(rename = "image/tiff")]
    ImageTiff,
    #[serde(rename = "image/bmp")]
    ImageBmp,
    #[serde(rename = "image/avif")]
    ImageAvif,
    #[serde(rename = "image/heic")]
    ImageHeic,
    #[serde(rename = "audio/mpeg")]
    AudioMpeg,
    #[serde(rename = "audio/mp3")]
    AudioMp3,
    #[serde(rename = "audio/wav")]
    AudioWav,
    #[serde(rename = "audio/ogg")]
    AudioOgg,
    #[serde(rename = "audio/oga")]
    AudioOga,
    #[serde(rename = "audio/aac")]
    AudioAac,
    #[serde(rename = "audio/mp4")]
    AudioMp4,
    #[serde(rename = "audio/flac")]
    AudioFlac,
    #[serde(rename = "audio/opus")]
    AudioOpus,
    #[serde(rename = "audio/webm")]
    AudioWebm,
    #[serde(rename = "video/mp4")]
    VideoMp4,
    #[serde(rename = "video/webm")]
    VideoWebm,
    #[serde(rename = "video/ogg")]
    VideoOgg,
    #[serde(rename = "video/mpeg")]
    VideoMpeg,
    #[serde(rename = "video/quicktime")]
    VideoQuicktime,
    #[serde(rename = "video/x-msvideo")]
    VideoXMsvideo,
    #[serde(rename = "video/x-matroska")]
    VideoXMatroska,
    #[serde(rename = "text/plain")]
    TextPlain,
    #[serde(rename = "text/html")]
    TextHtml,
    #[serde(rename = "text/css")]
    TextCss,
    #[serde(rename = "text/csv")]
    TextCsv,
    #[serde(rename = "text/markdown")]
    TextMarkdown,
    #[serde(rename = "text/x-python")]
    TextXPython,
    #[serde(rename = "application/javascript")]
    ApplicationJavascript,
    #[serde(rename = "text/x-typescript")]
    TextXTypescript,
    #[serde(rename = "application/x-yaml")]
    ApplicationXYaml,
    #[serde(rename = "application/pdf")]
    ApplicationPdf,
    #[serde(rename = "application/msword")]
    ApplicationMsword,
    #[serde(rename = "application/vnd.ms-excel")]
    ApplicationVndMsExcel,
    #[serde(rename = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")]
    ApplicationVndOpenxmlformatsOfficedocumentSpreadsheetmlSheet,
    #[serde(rename = "application/zip")]
    ApplicationZip,
    #[serde(rename = "application/json")]
    ApplicationJson,
    #[serde(rename = "application/xml")]
    ApplicationXml,
    #[serde(rename = "application/octet-stream")]
    ApplicationOctetStream,
    #[serde(rename = "application/vnd.openxmlformats-officedocument.wordprocessingml.document")]
    ApplicationVndOpenxmlformatsOfficedocumentWordprocessingmlDocument,
    #[serde(rename = "application/vnd.openxmlformats-officedocument.presentationml.presentation")]
    ApplicationVndOpenxmlformatsOfficedocumentPresentationmlPresentation,
    #[serde(rename = "application/rtf")]
    ApplicationRtf,
    #[serde(rename = "application/x-ndjson")]
    ApplicationXNdjson,
    #[serde(rename = "application/vnd.apache.parquet")]
    ApplicationVndApacheParquet,
    #[serde(rename = "application/gzip")]
    ApplicationGzip,
    #[serde(rename = "application/x-tar")]
    ApplicationXTar,
    #[serde(rename = "application/x-7z-compressed")]
    ApplicationX7zCompressed,
}
