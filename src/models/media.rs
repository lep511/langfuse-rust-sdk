use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaResponse {
    pub media_id: String,
    pub content_type: String,
    pub content_length: i64,
    pub uploaded_at: DateTime<Utc>,
    pub url: String,
    pub url_expiry: DateTime<Utc>,
}
