//! Error types returned by every SDK method.

use std::time::Duration;

use reqwest::StatusCode;

/// Errors returned by all [`LangfuseClient`](crate::LangfuseClient) methods.
///
/// HTTP status codes are mapped to dedicated variants so callers can match on
/// specific failure modes (rate-limiting, not-found, conflict, …).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LangfuseError {
    /// HTTP 400 — the request was malformed or invalid.
    #[error("Bad request (400): {message}")]
    BadRequest {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 401 — missing or invalid API credentials.
    #[error("Unauthorized (401): {message}")]
    Unauthorized {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 403 — the authenticated user lacks permission.
    #[error("Access denied (403): {message}")]
    AccessDenied {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 404 — the requested resource does not exist.
    #[error("Not found (404): {message}")]
    NotFound {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 405 — the HTTP method is not supported on this endpoint.
    #[error("Method not allowed (405): {message}")]
    MethodNotAllowed {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 409 — a conflicting resource already exists.
    #[error("Conflict (409): {message}")]
    Conflict {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 412 — a precondition (e.g. `If-Match`) was not met.
    #[error("Precondition failed (412): {message}")]
    PreconditionFailed {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 429 — too many requests; the retry policy may handle this automatically.
    #[error("Rate limited (429): retry after {retry_after:?}")]
    RateLimited {
        /// Server-suggested delay parsed from the `Retry-After` header.
        retry_after: Option<Duration>,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// HTTP 503 — the service is temporarily unavailable.
    #[error("Service unavailable (503): {message}")]
    ServiceUnavailable {
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// Other 5xx server errors.
    #[error("Server error ({status}): {message}")]
    ServerError {
        /// The HTTP status code.
        status: u16,
        /// Human-readable error message from the server.
        message: String,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// A network-level transport error (DNS, TLS, timeout, …).
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// Failed to serialize a request body or deserialize a response body.
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid [`ClientConfig`](crate::ClientConfig) values.
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// A URL could not be parsed.
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

impl LangfuseError {
    /// The parsed JSON body of an HTTP error response, if the server sent one.
    pub fn body(&self) -> Option<&serde_json::Value> {
        match self {
            LangfuseError::BadRequest { body, .. }
            | LangfuseError::Unauthorized { body, .. }
            | LangfuseError::AccessDenied { body, .. }
            | LangfuseError::NotFound { body, .. }
            | LangfuseError::MethodNotAllowed { body, .. }
            | LangfuseError::Conflict { body, .. }
            | LangfuseError::PreconditionFailed { body, .. }
            | LangfuseError::RateLimited { body, .. }
            | LangfuseError::ServiceUnavailable { body, .. }
            | LangfuseError::ServerError { body, .. } => body.as_ref(),
            LangfuseError::Transport(_)
            | LangfuseError::Serialization(_)
            | LangfuseError::Config(_)
            | LangfuseError::UrlParse(_) => None,
        }
    }
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
