//! Error types returned by every SDK method.
//!
//! An HTTP error keeps its status code ([`LangfuseError::status`]), a message
//! ([`LangfuseError::message`]: the body's `message` or `error` field, the
//! start of a body that is not JSON, or the status reason when the body is
//! empty) and the parsed JSON body ([`LangfuseError::body`]). Response headers
//! are available through a
//! [`response_hook`](crate::RequestOptions::with_response_hook).

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
    #[error("Rate limited (429): {message} (retry after {retry_after:?})")]
    RateLimited {
        /// Human-readable error message from the server.
        message: String,
        /// Server-suggested delay parsed from the `Retry-After` header.
        retry_after: Option<Duration>,
        /// Raw JSON error body, if the server returned one.
        body: Option<serde_json::Value>,
    },

    /// Other 4xx client errors (e.g. 402, 413, or 422 when a query exceeds a
    /// resource limit).
    #[error("Client error ({status}): {message}")]
    ClientError {
        /// The HTTP status code.
        status: u16,
        /// Human-readable error message from the server.
        message: String,
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

    /// Other 5xx server errors (and any other non-success status).
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

    /// Invalid media content, or a failed upload or download of media bytes.
    #[error("Media error: {0}")]
    Media(String),
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
            | LangfuseError::ClientError { body, .. }
            | LangfuseError::ServiceUnavailable { body, .. }
            | LangfuseError::ServerError { body, .. } => body.as_ref(),
            LangfuseError::Transport(_)
            | LangfuseError::Serialization(_)
            | LangfuseError::Config(_)
            | LangfuseError::UrlParse(_)
            | LangfuseError::Media(_) => None,
        }
    }

    /// The HTTP status code of an error response (also of a transport error
    /// that carries one).
    pub fn status(&self) -> Option<u16> {
        match self {
            LangfuseError::BadRequest { .. } => Some(400),
            LangfuseError::Unauthorized { .. } => Some(401),
            LangfuseError::AccessDenied { .. } => Some(403),
            LangfuseError::NotFound { .. } => Some(404),
            LangfuseError::MethodNotAllowed { .. } => Some(405),
            LangfuseError::Conflict { .. } => Some(409),
            LangfuseError::PreconditionFailed { .. } => Some(412),
            LangfuseError::RateLimited { .. } => Some(429),
            LangfuseError::ServiceUnavailable { .. } => Some(503),
            LangfuseError::ClientError { status, .. }
            | LangfuseError::ServerError { status, .. } => Some(*status),
            LangfuseError::Transport(err) => err.status().map(|s| s.as_u16()),
            LangfuseError::Serialization(_)
            | LangfuseError::Config(_)
            | LangfuseError::UrlParse(_)
            | LangfuseError::Media(_) => None,
        }
    }

    /// The server's message of an error response.
    pub fn message(&self) -> Option<&str> {
        match self {
            LangfuseError::BadRequest { message, .. }
            | LangfuseError::Unauthorized { message, .. }
            | LangfuseError::AccessDenied { message, .. }
            | LangfuseError::NotFound { message, .. }
            | LangfuseError::MethodNotAllowed { message, .. }
            | LangfuseError::Conflict { message, .. }
            | LangfuseError::PreconditionFailed { message, .. }
            | LangfuseError::RateLimited { message, .. }
            | LangfuseError::ClientError { message, .. }
            | LangfuseError::ServiceUnavailable { message, .. }
            | LangfuseError::ServerError { message, .. } => Some(message),
            LangfuseError::Transport(_)
            | LangfuseError::Serialization(_)
            | LangfuseError::Config(_)
            | LangfuseError::UrlParse(_)
            | LangfuseError::Media(_) => None,
        }
    }
}

/// Longest part of a non-JSON error body kept in the message.
const MAX_TEXT_MESSAGE_CHARS: usize = 500;

pub(crate) fn map_response_error(
    status: StatusCode,
    body_bytes: &[u8],
    headers: &reqwest::header::HeaderMap,
) -> LangfuseError {
    let body: Option<serde_json::Value> = serde_json::from_slice(body_bytes).ok();
    let message = error_message(status, body.as_ref(), body_bytes);

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
            LangfuseError::RateLimited {
                message,
                retry_after,
                body,
            }
        }
        status @ 400..=499 => LangfuseError::ClientError {
            status,
            message,
            body,
        },
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

/// The message of an error response: the `message` field of a JSON body, its
/// `error` field (a string, or an object with a `message`), the compact JSON
/// of another body, the (trimmed, shortened) text of a body that is not JSON,
/// or the status reason of an empty body.
fn error_message(status: StatusCode, body: Option<&serde_json::Value>, raw: &[u8]) -> String {
    if let Some(body) = body {
        let field = |value: &serde_json::Value, key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let message = field(body, "message")
            .or_else(|| field(body, "error"))
            .or_else(|| body.get("error").and_then(|e| field(e, "message")))
            .or_else(|| body.as_str().filter(|s| !s.is_empty()).map(str::to_owned));
        return message.unwrap_or_else(|| shorten(&body.to_string()));
    }
    let text = String::from_utf8_lossy(raw);
    let text = text.trim();
    if text.is_empty() {
        return status
            .canonical_reason()
            .unwrap_or("Unknown error")
            .to_owned();
    }
    shorten(text)
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT_MESSAGE_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}
