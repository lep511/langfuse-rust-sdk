use std::time::Duration;

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LangfuseError {
    #[error("Bad request (400): {message}")]
    BadRequest {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Unauthorized (401): {message}")]
    Unauthorized {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Access denied (403): {message}")]
    AccessDenied {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Not found (404): {message}")]
    NotFound {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Method not allowed (405): {message}")]
    MethodNotAllowed {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Conflict (409): {message}")]
    Conflict {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Precondition failed (412): {message}")]
    PreconditionFailed {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Rate limited (429): retry after {retry_after:?}")]
    RateLimited {
        retry_after: Option<Duration>,
        body: Option<serde_json::Value>,
    },

    #[error("Service unavailable (503): {message}")]
    ServiceUnavailable {
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("Server error ({status}): {message}")]
    ServerError {
        status: u16,
        message: String,
        body: Option<serde_json::Value>,
    },

    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

pub(crate) fn map_response_error(
    status: StatusCode,
    body_bytes: &[u8],
    headers: &reqwest::header::HeaderMap,
) -> LangfuseError {
    let body: Option<serde_json::Value> = serde_json::from_slice(body_bytes).ok();
    let message = body
        .as_ref()
        .and_then(|b| b.get("message").and_then(|m| m.as_str()))
        .unwrap_or("Unknown error")
        .to_string();

    match status.as_u16() {
        400 => LangfuseError::BadRequest { message, body },
        401 => LangfuseError::Unauthorized { message, body },
        403 => LangfuseError::AccessDenied { message, body },
        404 => LangfuseError::NotFound { message, body },
        405 => LangfuseError::MethodNotAllowed { message, body },
        409 => LangfuseError::Conflict { message, body },
        412 => LangfuseError::PreconditionFailed { message, body },
        429 => {
            let retry_after = parse_retry_after(headers);
            LangfuseError::RateLimited { retry_after, body }
        }
        503 => LangfuseError::ServiceUnavailable { message, body },
        status @ 500..=599 => LangfuseError::ServerError {
            status,
            message,
            body,
        },
        _ => LangfuseError::ServerError {
            status: status.as_u16(),
            message,
            body,
        },
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}
