//! Media attachments (images, audio, PDFs, ...) for traces and dataset items.
//!
//! A [`LangfuseMedia`] holds the bytes and MIME type of one file. Langfuse
//! stores the file once (keyed by its SHA-256) and references it from trace
//! data with a string like
//! `@@@langfuseMedia:type=image/png|id=<media id>|source=bytes@@@`, which the UI
//! renders as the file.
//!
//! - [`LangfuseMedia::upload`] uploads a file and links it to a trace,
//!   observation or dataset item field.
//! - With the `otel` feature, base64 data URIs (and the base64 shapes of common
//!   LLM APIs) in span attributes are uploaded automatically during export and
//!   replaced by their reference string. `LangfuseMedia` serializes as a data
//!   URI, so `span.set_input(&json!({"image": media}))` is uploaded too.
//! - [`MediaClient::resolve_references`](crate::api::media::MediaClient::resolve_references)
//!   turns reference strings in fetched data back into data URIs.
//!
//! ```
//! use langfuse_rust_sdk::media::{LangfuseMedia, MediaReference};
//! use langfuse_rust_sdk::models::media::MediaContentType;
//!
//! let media = LangfuseMedia::from_bytes(b"\x89PNG...".to_vec(), MediaContentType::ImagePng);
//! let reference = media.reference_string();
//! assert!(reference.starts_with("@@@langfuseMedia:type=image/png|id="));
//! assert_eq!(MediaReference::parse(&reference).unwrap().media_id, media.media_id());
//! ```

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use reqwest::StatusCode;
use serde::{Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::LangfuseClient;
use crate::api::media::MediaClient;
use crate::error::LangfuseError;
use crate::models::media::{GetMediaUploadUrlRequest, MediaContentType, PatchMediaBody};

const REFERENCE_PREFIX: &str = "@@@langfuseMedia:";
const REFERENCE_SUFFIX: &str = "@@@";
/// How deep [`extract_media`] and reference resolution look into JSON.
pub(crate) const MAX_DEPTH: usize = 10;
/// Longest data URI header (`data:<mime>;base64`) recognized.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
const MAX_DATA_URI_HEADER: usize = 256;

/// How the media was provided; part of the reference string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaSource {
    /// From a base64 data URI.
    Base64DataUri,
    /// From raw bytes.
    Bytes,
    /// From a file.
    File,
}

impl MediaSource {
    /// Wire name (`base64_data_uri`, `bytes`, `file`).
    pub fn as_str(self) -> &'static str {
        match self {
            MediaSource::Base64DataUri => "base64_data_uri",
            MediaSource::Bytes => "bytes",
            MediaSource::File => "file",
        }
    }
}

/// One media file: its bytes, MIME type and content hash.
#[derive(Clone)]
pub struct LangfuseMedia {
    content_type: MediaContentType,
    bytes: Arc<[u8]>,
    source: MediaSource,
    sha256_hash: String,
    media_id: String,
}

impl fmt::Debug for LangfuseMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseMedia")
            .field("content_type", &self.content_type)
            .field("content_length", &self.bytes.len())
            .field("source", &self.source)
            .field("media_id", &self.media_id)
            .finish()
    }
}

impl LangfuseMedia {
    fn new(bytes: Vec<u8>, content_type: MediaContentType, source: MediaSource) -> Self {
        let digest = Sha256::digest(&bytes);
        let sha256_hash = STANDARD.encode(digest);
        // URL-safe form of the hash, truncated: the ID Langfuse derives too.
        let media_id: String = sha256_hash
            .chars()
            .map(|c| match c {
                '+' => '-',
                '/' => '_',
                other => other,
            })
            .take(22)
            .collect();
        Self {
            content_type,
            bytes: bytes.into(),
            source,
            sha256_hash,
            media_id,
        }
    }

    /// Media from raw bytes.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>, content_type: MediaContentType) -> Self {
        Self::new(bytes.into(), content_type, MediaSource::Bytes)
    }

    /// Media from a base64 data URI (`data:image/png;base64,iVBOR...`).
    ///
    /// Returns [`LangfuseError::Media`] if the URI is malformed, not base64 or
    /// of a content type Langfuse does not accept.
    pub fn from_data_uri(uri: &str) -> Result<Self, LangfuseError> {
        let invalid = |reason: &str| LangfuseError::Media(format!("invalid data URI: {reason}"));
        let rest = uri
            .strip_prefix("data:")
            .ok_or_else(|| invalid("missing 'data:' prefix"))?;
        let (header, data) = rest.split_once(',').ok_or_else(|| invalid("missing ','"))?;
        let mut parts = header.split(';');
        let mime = parts.next().unwrap_or_default();
        if !parts.any(|p| p == "base64") {
            return Err(invalid("not base64-encoded"));
        }
        if mime.is_empty() || data.is_empty() {
            return Err(invalid("empty content type or data"));
        }
        let content_type = MediaContentType::from_mime(mime)
            .ok_or_else(|| LangfuseError::Media(format!("unsupported media type '{mime}'")))?;
        // Also accept the URL-safe alphabet and missing padding.
        let normalized: String = data
            .trim()
            .chars()
            .map(|c| match c {
                '-' => '+',
                '_' => '/',
                other => other,
            })
            .collect();
        let padded = match normalized.len() % 4 {
            2 => format!("{normalized}=="),
            3 => format!("{normalized}="),
            _ => normalized,
        };
        let bytes = STANDARD
            .decode(padded.as_bytes())
            .map_err(|e| invalid(&format!("bad base64: {e}")))?;
        Ok(Self::new(bytes, content_type, MediaSource::Base64DataUri))
    }

    /// Media from a file. Returns [`LangfuseError::Media`] if it cannot be read.
    pub fn from_file(
        path: impl AsRef<Path>,
        content_type: MediaContentType,
    ) -> Result<Self, LangfuseError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| LangfuseError::Media(format!("cannot read {}: {e}", path.display())))?;
        Ok(Self::new(bytes, content_type, MediaSource::File))
    }

    /// The MIME type.
    pub fn content_type(&self) -> MediaContentType {
        self.content_type
    }

    /// The content.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The content size in bytes.
    pub fn content_length(&self) -> usize {
        self.bytes.len()
    }

    /// Base64 SHA-256 of the content (sent as `x-amz-checksum-sha256`).
    pub fn sha256_hash(&self) -> &str {
        &self.sha256_hash
    }

    /// The media ID: the first 22 characters of the URL-safe SHA-256.
    pub fn media_id(&self) -> &str {
        &self.media_id
    }

    /// How the media was provided.
    pub fn source(&self) -> MediaSource {
        self.source
    }

    /// The string that stands for this media in trace data.
    pub fn reference_string(&self) -> String {
        MediaReference {
            media_id: self.media_id.clone(),
            content_type: self.content_type.mime(),
            source: self.source.as_str().to_owned(),
        }
        .to_string()
    }

    /// The content as a base64 data URI.
    pub fn to_data_uri(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.content_type.mime(),
            STANDARD.encode(&self.bytes)
        )
    }

    /// Upload the media and link it to `target`.
    ///
    /// Requests a presigned URL, uploads the bytes there (with retries on
    /// network errors, 429 and 5xx) and reports the outcome to Langfuse. Media
    /// whose content Langfuse already has is not uploaded again.
    pub async fn upload(
        &self,
        client: &LangfuseClient,
        target: &MediaTarget,
    ) -> Result<MediaUploadStatus, LangfuseError> {
        self.upload_with(&client.media, target).await
    }

    pub(crate) async fn upload_with(
        &self,
        media: &MediaClient,
        target: &MediaTarget,
    ) -> Result<MediaUploadStatus, LangfuseError> {
        let mut request = GetMediaUploadUrlRequest::new(
            self.content_type,
            self.content_length() as i64,
            self.sha256_hash.clone(),
            target.field.as_str(),
        );
        request.trace_id = target.trace_id.clone();
        request.observation_id = target.observation_id.clone();
        request.dataset_id = target.dataset_id.clone();
        request.dataset_item_id = target.dataset_item_id.clone();
        let response = media.get_upload_url(&request).await?;
        let Some(upload_url) = response.upload_url else {
            return Ok(MediaUploadStatus::AlreadyUploaded);
        };
        if response.media_id != self.media_id {
            return Err(LangfuseError::Media(format!(
                "media ID mismatch: computed {}, server returned {}; upload cancelled",
                self.media_id, response.media_id
            )));
        }

        let started = Instant::now();
        let (status, body) = self.put_presigned(media, &upload_url).await?;
        let upload_time_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let succeeded = status.is_success();
        media
            .patch(
                &self.media_id,
                &PatchMediaBody {
                    uploaded_at: Utc::now(),
                    upload_http_status: i64::from(status.as_u16()),
                    upload_http_error: (!succeeded).then(|| body.clone()),
                    upload_time_ms: Some(upload_time_ms),
                },
            )
            .await?;
        if succeeded {
            Ok(MediaUploadStatus::Uploaded)
        } else {
            Err(LangfuseError::Media(format!(
                "upload to storage failed with HTTP {status}: {body}"
            )))
        }
    }

    /// PUT the bytes to the presigned URL, without Langfuse credentials.
    async fn put_presigned(
        &self,
        media: &MediaClient,
        url: &str,
    ) -> Result<(StatusCode, String), LangfuseError> {
        let http = &media.http;
        let policy = http.retry_policy();
        let mime = self.content_type.mime();
        let mut attempt = 0;
        loop {
            let mut request = http
                .raw_client()
                .put(url)
                .header(reqwest::header::CONTENT_TYPE, &mime)
                .body(self.bytes.to_vec());
            // Self-hosted Google Cloud Storage rejects these headers.
            if !url.contains("storage.googleapis.com") {
                request = request
                    .header("x-ms-blob-type", "BlockBlob")
                    .header("x-amz-checksum-sha256", &self.sha256_hash);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let retryable =
                        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                    if !retryable || attempt >= policy.max_retries {
                        let body = response.text().await.unwrap_or_default();
                        return Ok((status, body));
                    }
                }
                // The PUT is idempotent, so every network error is retried.
                Err(err) if attempt < policy.max_retries && !err.is_builder() => {}
                Err(err) => return Err(LangfuseError::Transport(err)),
            }
            tokio::time::sleep(policy.delay(attempt, None)).await;
            attempt += 1;
        }
    }
}

/// Serializes as a base64 data URI, so media inside traced data is detected
/// and uploaded by the `otel` exporter.
impl Serialize for LangfuseMedia {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_data_uri())
    }
}

/// Outcome of [`LangfuseMedia::upload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaUploadStatus {
    /// The bytes were uploaded.
    Uploaded,
    /// Langfuse already had this content; only the link was recorded.
    AlreadyUploaded,
}

/// Where uploaded media is linked: a trace or observation field, or a dataset
/// item field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTarget {
    trace_id: Option<String>,
    observation_id: Option<String>,
    dataset_id: Option<String>,
    dataset_item_id: Option<String>,
    field: MediaField,
}

impl MediaTarget {
    /// A field (`Input`, `Output` or `Metadata`) of a trace.
    pub fn trace(trace_id: impl Into<String>, field: MediaField) -> Self {
        Self {
            trace_id: Some(trace_id.into()),
            observation_id: None,
            dataset_id: None,
            dataset_item_id: None,
            field,
        }
    }

    /// A field (`Input`, `Output` or `Metadata`) of an observation.
    pub fn observation(
        trace_id: impl Into<String>,
        observation_id: impl Into<String>,
        field: MediaField,
    ) -> Self {
        Self {
            observation_id: Some(observation_id.into()),
            ..Self::trace(trace_id, field)
        }
    }

    /// A field (`Input`, `ExpectedOutput` or `Metadata`) of a dataset item.
    pub fn dataset_item(
        dataset_id: impl Into<String>,
        dataset_item_id: impl Into<String>,
        field: MediaField,
    ) -> Self {
        Self {
            trace_id: None,
            observation_id: None,
            dataset_id: Some(dataset_id.into()),
            dataset_item_id: Some(dataset_item_id.into()),
            field,
        }
    }
}

/// The field media is linked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaField {
    /// `input`
    Input,
    /// `output` (traces and observations)
    Output,
    /// `metadata`
    Metadata,
    /// `expectedOutput` (dataset items)
    ExpectedOutput,
}

impl MediaField {
    /// Wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            MediaField::Input => "input",
            MediaField::Output => "output",
            MediaField::Metadata => "metadata",
            MediaField::ExpectedOutput => "expectedOutput",
        }
    }
}

/// A parsed `@@@langfuseMedia:type=...|id=...|source=...@@@` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaReference {
    /// Media ID.
    pub media_id: String,
    /// MIME type.
    pub content_type: String,
    /// How the media was provided (`base64_data_uri`, `bytes`, `file`).
    pub source: String,
}

impl MediaReference {
    /// Parse a reference string. Returns [`LangfuseError::Media`] if it is not
    /// one or lacks `type`, `id` or `source`.
    pub fn parse(reference: &str) -> Result<Self, LangfuseError> {
        let invalid =
            |reason: &str| LangfuseError::Media(format!("invalid media reference: {reason}"));
        if !reference.starts_with("@@@langfuseMedia:type=") {
            return Err(invalid("must start with '@@@langfuseMedia:type='"));
        }
        let content = reference
            .strip_prefix(REFERENCE_PREFIX)
            .and_then(|r| r.strip_suffix(REFERENCE_SUFFIX))
            .ok_or_else(|| invalid("must end with '@@@'"))?;
        let (mut media_id, mut content_type, mut source) = (None, None, None);
        for pair in content.split('|') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| invalid("malformed key=value pair"))?;
            match key {
                "id" => media_id = Some(value.to_owned()),
                "type" => content_type = Some(value.to_owned()),
                "source" => source = Some(value.to_owned()),
                _ => {}
            }
        }
        match (media_id, content_type, source) {
            (Some(media_id), Some(content_type), Some(source)) => Ok(Self {
                media_id,
                content_type,
                source,
            }),
            _ => Err(invalid("missing type, id or source")),
        }
    }
}

impl fmt::Display for MediaReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{REFERENCE_PREFIX}type={}|id={}|source={}{REFERENCE_SUFFIX}",
            self.content_type, self.media_id, self.source
        )
    }
}

/// Every `@@@langfuseMedia:...@@@` substring of `text`.
pub(crate) fn find_references(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find(REFERENCE_PREFIX).map(|i| from + i) {
        let body = start + REFERENCE_PREFIX.len();
        // At least one character between prefix and suffix.
        let Some(end) = text
            .get(body + 1..)
            .and_then(|rest| rest.find(REFERENCE_SUFFIX))
        else {
            break;
        };
        let end = body + 1 + end + REFERENCE_SUFFIX.len();
        out.push(&text[start..end]);
        from = end;
    }
    out
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
fn is_reference(text: &str) -> bool {
    text.starts_with(REFERENCE_PREFIX) && text.ends_with(REFERENCE_SUFFIX)
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn is_base64_data_uri(text: &str) -> bool {
    text.starts_with("data:")
        && text
            .get(..text.len().min(MAX_DATA_URI_HEADER))
            .and_then(|head| head.find(','))
            .is_some_and(|comma| text[..comma].ends_with(";base64"))
}

/// Replace base64 media in `value` by reference strings, returning the media
/// to upload. Recognized shapes (up to 10 levels deep):
///
/// - data URI strings (`data:image/png;base64,...`);
/// - `{"type": "base64", "media_type": .., "data": ..}` (Anthropic);
/// - `{"type": "media", "mime_type": .., "data": ..}`;
/// - `{"inline_data" | "inlineData": {"mime_type" | "mimeType": .., "data": ..}}` (Gemini).
///
/// Media of unsupported types or with invalid base64 is left as-is.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn extract_media(value: Value, found: &mut Vec<LangfuseMedia>) -> Value {
    walk(value, 0, found)
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
fn walk(value: Value, depth: usize, found: &mut Vec<LangfuseMedia>) -> Value {
    if depth > MAX_DEPTH {
        return value;
    }
    match value {
        Value::String(text) => match media_from_data_uri(&text) {
            Some(media) => {
                let reference = media.reference_string();
                found.push(media);
                Value::String(reference)
            }
            None => Value::String(text),
        },
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| walk(v, depth + 1, found))
                .collect(),
        ),
        Value::Object(mut map) => {
            // Base64 block with a MIME field next to `data`.
            let mime_key = match map.get("type").and_then(Value::as_str) {
                Some("base64") => Some("media_type"),
                Some("media") => Some("mime_type"),
                _ => None,
            };
            if let Some(mime_key) = mime_key
                && let Some(reference) = replace_base64_field(&mut map, mime_key, found)
            {
                map.insert("data".into(), Value::String(reference));
                return Value::Object(map);
            }
            for inline_key in ["inline_data", "inlineData"] {
                if let Some(Value::Object(inline)) = map.get_mut(inline_key) {
                    let mime_key = if inline.contains_key("mime_type") {
                        "mime_type"
                    } else {
                        "mimeType"
                    };
                    if let Some(reference) = replace_base64_field(inline, mime_key, found) {
                        inline.insert("data".into(), Value::String(reference));
                        return Value::Object(map);
                    }
                }
            }
            Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, walk(v, depth + 1, found)))
                    .collect(),
            )
        }
        other => other,
    }
}

/// For an object with `<mime_key>` and base64 `data`, build the media and
/// return its reference string.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
fn replace_base64_field(
    map: &mut serde_json::Map<String, Value>,
    mime_key: &str,
    found: &mut Vec<LangfuseMedia>,
) -> Option<String> {
    let mime = map.get(mime_key)?.as_str()?;
    let data = map.get("data")?.as_str()?;
    if is_reference(data) {
        return None;
    }
    let media = media_from_data_uri(&format!("data:{mime};base64,{data}"))?;
    let reference = media.reference_string();
    found.push(media);
    Some(reference)
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
fn media_from_data_uri(text: &str) -> Option<LangfuseMedia> {
    if !is_base64_data_uri(text) {
        return None;
    }
    match LangfuseMedia::from_data_uri(text) {
        Ok(media) => Some(media),
        Err(err) => {
            tracing::debug!(error = %err, "data URI not uploaded as media");
            None
        }
    }
}

/// Cheap check before parsing a string attribute as JSON.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
fn may_contain_media(text: &str) -> bool {
    text.contains("data:")
        || text.contains("inline_data")
        || text.contains("inlineData")
        || ((text.contains("\"media_type\"")
            || text.contains("\"mime_type\"")
            || text.contains("\"mimeType\""))
            && text.contains("\"data\""))
}

/// Process one string span attribute: a data URI becomes its reference string,
/// and JSON containing media is rewritten with references. `None` when the
/// value has no media.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn extract_from_attribute(text: &str, found: &mut Vec<LangfuseMedia>) -> Option<String> {
    if let Some(media) = media_from_data_uri(text) {
        let reference = media.reference_string();
        found.push(media);
        return Some(reference);
    }
    let trimmed = text.trim_start();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[') || trimmed.starts_with('"'))
        || !may_contain_media(text)
    {
        return None;
    }
    let parsed: Value = serde_json::from_str(text).ok()?;
    let before = found.len();
    let processed = extract_media(parsed, found);
    (found.len() > before).then(|| processed.to_string())
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
const INPUT_ATTRIBUTE_KEYS: &[&str] = &[
    "langfuse.observation.input",
    "ai.prompt.messages",
    "ai.prompt",
    "ai.toolCall.args",
    "gcp.vertex.agent.llm_request",
    "gcp.vertex.agent.tool_call_args",
    "prompt",
    "lk.input_text",
    "lk.user_transcript",
    "lk.chat_ctx",
    "lk.user_input",
    "mlflow.spanInputs",
    "traceloop.entity.input",
    "input.value",
    "pydantic_ai.all_messages",
    "gen_ai.system_instructions",
    "input",
    "gen_ai.input.messages",
    "gen_ai.tool.call.arguments",
    "genkit:input",
    "tool_arguments",
];

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
const OUTPUT_ATTRIBUTE_KEYS: &[&str] = &[
    "langfuse.observation.output",
    "ai.response.text",
    "ai.result.text",
    "ai.toolCall.result",
    "ai.response.object",
    "ai.result.object",
    "ai.response.toolCalls",
    "ai.result.toolCalls",
    "gcp.vertex.agent.llm_response",
    "gcp.vertex.agent.tool_response",
    "all_messages_events",
    "lk.function_tool.output",
    "lk.response.text",
    "mlflow.spanOutputs",
    "traceloop.entity.output",
    "output.value",
    "final_result",
    "output",
    "gen_ai.output.messages",
    "gen_ai.tool.call.result",
    "genkit:output",
    "tool_response",
];

/// The media field for media found in span attribute `key`.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn field_for_attribute(key: &str) -> MediaField {
    if INPUT_ATTRIBUTE_KEYS.contains(&key)
        || key.starts_with("gen_ai.prompt")
        || key.starts_with("llm.input_messages")
    {
        MediaField::Input
    } else if OUTPUT_ATTRIBUTE_KEYS.contains(&key)
        || key.starts_with("gen_ai.completion")
        || key.starts_with("llm.output_messages")
    {
        MediaField::Output
    } else {
        MediaField::Metadata
    }
}

/// Timeout of each download in reference resolution.
pub(crate) const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PNG_B64: &str = "iVBORw0KGgo=";

    #[test]
    fn hash_and_media_id_are_derived_from_the_content() {
        let media = LangfuseMedia::from_bytes(b"hello".to_vec(), MediaContentType::TextPlain);
        // base64(sha256("hello")) = LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=
        assert_eq!(
            media.sha256_hash(),
            "LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ="
        );
        assert_eq!(media.media_id(), "LPJNul-wow4m6Dsqxbninh");
        assert_eq!(media.media_id().len(), 22);
        assert_eq!(media.content_length(), 5);
        assert_eq!(
            media.reference_string(),
            "@@@langfuseMedia:type=text/plain|id=LPJNul-wow4m6Dsqxbninh|source=bytes@@@"
        );
    }

    #[test]
    fn data_uris_round_trip() {
        let uri = format!("data:image/png;base64,{PNG_B64}");
        let media = LangfuseMedia::from_data_uri(&uri).unwrap();
        assert_eq!(media.content_type(), MediaContentType::ImagePng);
        assert_eq!(media.source(), MediaSource::Base64DataUri);
        assert_eq!(media.bytes(), b"\x89PNG\r\n\x1a\n");
        assert_eq!(media.to_data_uri(), uri);
        assert_eq!(serde_json::to_value(&media).unwrap(), json!(uri));
        // URL-safe alphabet and missing padding are accepted.
        assert_eq!(
            LangfuseMedia::from_data_uri("data:image/png;base64,iVBORw0KGgo")
                .unwrap()
                .bytes(),
            media.bytes()
        );
    }

    #[test]
    fn invalid_data_uris_are_rejected() {
        for uri in [
            "image/png;base64,abc",
            "data:image/png,abc",
            "data:;base64,abc",
            "data:image/png;base64,",
            "data:application/x-unknown;base64,abc",
            "data:image/png;base64,!!!",
        ] {
            assert!(
                matches!(
                    LangfuseMedia::from_data_uri(uri),
                    Err(LangfuseError::Media(_))
                ),
                "{uri}"
            );
        }
    }

    #[test]
    fn references_parse_and_display() {
        let reference = "@@@langfuseMedia:type=image/jpeg|id=abc_123|source=bytes@@@";
        let parsed = MediaReference::parse(reference).unwrap();
        assert_eq!(parsed.media_id, "abc_123");
        assert_eq!(parsed.content_type, "image/jpeg");
        assert_eq!(parsed.source, "bytes");
        assert_eq!(parsed.to_string(), reference);
        for bad in [
            "",
            "@@@langfuseMedia:id=1@@@",
            "@@@langfuseMedia:type=a|id=1",
            "@@@langfuseMedia:type=a|id=1@@@",
        ] {
            assert!(MediaReference::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn finds_references_inside_text() {
        let text = "a @@@langfuseMedia:type=image/png|id=1|source=bytes@@@ b @@@langfuseMedia:type=x|id=2|source=file@@@";
        assert_eq!(
            find_references(text),
            [
                "@@@langfuseMedia:type=image/png|id=1|source=bytes@@@",
                "@@@langfuseMedia:type=x|id=2|source=file@@@"
            ]
        );
        assert!(find_references("@@@langfuseMedia:@@@").is_empty());
    }

    #[test]
    fn extracts_every_supported_shape() {
        let uri = format!("data:image/png;base64,{PNG_B64}");
        let value = json!({
            "uri": uri,
            "anthropic": {"type": "base64", "media_type": "image/png", "data": PNG_B64},
            "generic": {"type": "media", "mime_type": "image/png", "data": PNG_B64},
            "gemini": {"inline_data": {"mime_type": "image/png", "data": PNG_B64}},
            "gemini_camel": {"inlineData": {"mimeType": "image/png", "data": PNG_B64}},
            "nested": [[{"deep": uri}]],
            "text": "no media here",
            "unsupported": "data:application/x-unknown;base64,AAAA"
        });
        let mut found = Vec::new();
        let out = extract_media(value, &mut found);
        assert_eq!(found.len(), 6);
        let reference = found[0].reference_string();
        assert_eq!(out["uri"], json!(reference));
        assert_eq!(out["anthropic"]["data"], json!(reference));
        assert_eq!(out["anthropic"]["media_type"], "image/png");
        assert_eq!(out["generic"]["data"], json!(reference));
        assert_eq!(out["gemini"]["inline_data"]["data"], json!(reference));
        assert_eq!(out["gemini_camel"]["inlineData"]["data"], json!(reference));
        assert_eq!(out["nested"][0][0]["deep"], json!(reference));
        assert_eq!(out["text"], "no media here");
        assert_eq!(out["unsupported"], "data:application/x-unknown;base64,AAAA");
    }

    #[test]
    fn already_referenced_media_is_left_alone() {
        let reference = "@@@langfuseMedia:type=image/png|id=x|source=bytes@@@";
        let mut found = Vec::new();
        let out = extract_media(
            json!({"type": "base64", "media_type": "image/png", "data": reference}),
            &mut found,
        );
        assert!(found.is_empty());
        assert_eq!(out["data"], reference);
    }

    #[test]
    fn attribute_strings_are_processed() {
        let uri = format!("data:image/png;base64,{PNG_B64}");
        let mut found = Vec::new();
        assert!(
            extract_from_attribute(&uri, &mut found)
                .unwrap()
                .starts_with("@@@langfuseMedia:")
        );
        let json_text = json!({"messages": [{"image": uri}]}).to_string();
        let rewritten = extract_from_attribute(&json_text, &mut found).unwrap();
        assert!(
            rewritten.contains("@@@langfuseMedia:type=image/png"),
            "{rewritten}"
        );
        // JSON-encoded string (metadata values).
        let quoted = Value::String(uri.clone()).to_string();
        assert!(
            extract_from_attribute(&quoted, &mut found)
                .unwrap()
                .starts_with("\"@@@langfuseMedia:")
        );
        assert!(extract_from_attribute("plain text", &mut found).is_none());
        assert!(extract_from_attribute(r#"{"a": 1}"#, &mut found).is_none());
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn attribute_fields() {
        assert_eq!(
            field_for_attribute("langfuse.observation.input"),
            MediaField::Input
        );
        assert_eq!(
            field_for_attribute("gen_ai.prompt.0.content"),
            MediaField::Input
        );
        assert_eq!(
            field_for_attribute("gen_ai.output.messages"),
            MediaField::Output
        );
        assert_eq!(
            field_for_attribute("langfuse.observation.metadata.image"),
            MediaField::Metadata
        );
    }
}
