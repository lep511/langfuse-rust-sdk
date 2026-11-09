use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::HttpClient;
use crate::models::media::GetMediaResponse;

pub struct MediaClient {
    pub(crate) http: Arc<HttpClient>,
}

impl MediaClient {
    pub async fn get(&self, media_id: &str) -> Result<GetMediaResponse, LangfuseError> {
        self.http
            .get(&format!("/api/public/media/{media_id}"))
            .await
    }
}
