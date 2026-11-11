//! `langfuse api media`.

use clap::Args;
use serde_json::Value;

use crate::args::{BodyChannel, ObjectBody, date_time};
use crate::error::Result;
use crate::operation::{Context, HttpMethod, Operation, OperationMeta, PreparedRequest};

path_op! {
    Get {
        /// The media ID
        media_id => "mediaId",
    }
    meta = OperationMeta::new(
        "media", "get", "media_get",
        HttpMethod::Get, "/api/public/media/{mediaId}",
    );
}

#[derive(Debug, Args)]
pub struct Update {
    /// The media ID
    media_id: String,
    #[command(flatten)]
    fields: UpdateFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UpdateFields {
    /// When the media was uploaded (RFC 3339)
    #[arg(long, value_parser = date_time, required_unless_present_any = ["body_json", "body_file"])]
    uploaded_at: Option<String>,
    /// HTTP status of the upload
    #[arg(long, value_name = "STATUS", required_unless_present_any = ["body_json", "body_file"])]
    upload_http_status: Option<i64>,
    /// HTTP error message of the upload
    #[arg(long)]
    upload_http_error: Option<String>,
    /// Upload duration in milliseconds
    #[arg(long, value_name = "MS")]
    upload_time_ms: Option<i64>,
}

impl Operation for Update {
    const META: OperationMeta = OperationMeta::new(
        "media",
        "update",
        "media_patch",
        HttpMethod::Patch,
        "/api/public/media/{mediaId}",
    )
    .empty()
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META).path_param("mediaId", &self.media_id);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        Ok(request.body(
            ObjectBody::new()
                .opt("uploadedAt", f.uploaded_at.clone())
                .opt("uploadHttpStatus", f.upload_http_status)
                .opt("uploadHttpError", f.upload_http_error.clone())
                .opt("uploadTimeMs", f.upload_time_ms)
                .build(),
        ))
    }
}

#[derive(Debug, Args)]
pub struct GetUploadUrl {
    #[command(flatten)]
    fields: UploadFields,
    #[command(flatten)]
    body: BodyChannel,
}

#[derive(Debug, Args)]
#[group(id = "body-fields", multiple = true, conflicts_with_all = ["body_json", "body_file"])]
#[command(next_help_heading = "Request body")]
struct UploadFields {
    /// Trace of the media (unset for dataset item media)
    #[arg(long)]
    trace_id: Option<String>,
    /// Observation of the media, when it belongs to one
    #[arg(long)]
    observation_id: Option<String>,
    /// Dataset of the media (unset for trace media)
    #[arg(long)]
    dataset_id: Option<String>,
    /// Dataset item of the media (need not exist yet)
    #[arg(long)]
    dataset_item_id: Option<String>,
    /// MIME type of the media (image/png, audio/mpeg, application/pdf, …)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    content_type: Option<String>,
    /// Size in bytes
    #[arg(long, value_name = "BYTES", required_unless_present_any = ["body_json", "body_file"])]
    content_length: Option<i64>,
    /// Base64 SHA-256 hash of the content
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    sha256_hash: Option<String>,
    /// Field the media is in: input, output, metadata (trace) or input, expectedOutput,
    /// metadata (dataset item)
    #[arg(long, required_unless_present_any = ["body_json", "body_file"])]
    field: Option<String>,
}

impl Operation for GetUploadUrl {
    const META: OperationMeta = OperationMeta::new(
        "media",
        "get-upload-url",
        "media_getUploadUrl",
        HttpMethod::Post,
        "/api/public/media",
    )
    .fields();

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let request = PreparedRequest::new(&Self::META);
        if let Some(body) = self.body.read(ctx)? {
            return Ok(request.body(body));
        }
        let f = &self.fields;
        let body: Value = ObjectBody::new()
            .opt("traceId", f.trace_id.clone())
            .opt("observationId", f.observation_id.clone())
            .opt("datasetId", f.dataset_id.clone())
            .opt("datasetItemId", f.dataset_item_id.clone())
            .opt("contentType", f.content_type.clone())
            .opt("contentLength", f.content_length)
            .opt("sha256Hash", f.sha256_hash.clone())
            .opt("field", f.field.clone())
            .build();
        Ok(request.body(body))
    }
}

resource! {
    pub enum Command {
        /// Get a media record
        Get(Get),
        /// Update a media record after its upload
        Update(Update),
        /// Get a presigned upload URL for a media record
        GetUploadUrl(GetUploadUrl),
    }
}
