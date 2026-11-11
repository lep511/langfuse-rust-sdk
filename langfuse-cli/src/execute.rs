//! Sending a [`PreparedRequest`] through the SDK's [`HttpClient`], and
//! rendering it as a `curl` command.

use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use langfuse_rust_sdk::http::HttpClient;
use langfuse_rust_sdk::{LangfuseError, RequestOptions};
use reqwest::header::HeaderMap;
use serde_json::{Map, Value};
use url::Url;

use crate::config::{Settings, USER_AGENT};
use crate::error::{CliError, Exit, Result, error_chain};
use crate::operation::{HttpMethod, PreparedRequest, ResponseKind};

/// Version of the SDK this CLI is built on (sent as `x-langfuse-sdk-version`).
pub const SDK_VERSION: &str = "0.11.3";

/// The outcome of an API call that reached the server.
#[derive(Debug, Clone)]
pub struct ApiResult {
    /// HTTP status of the last attempt.
    pub status: u16,
    /// Response headers (lowercase names; repeated headers joined with `, `).
    pub headers: Map<String, Value>,
    /// Whether the status is 2xx.
    pub ok: bool,
    /// The body: JSON, a string for non-JSON error bodies, or `None` when empty.
    pub body: Option<Value>,
}

/// The URL the SDK requests for `request`: the path joined to the host (a
/// base path prefix is kept), then the query.
pub fn request_url(host: &str, request: &PreparedRequest) -> Result<Url> {
    let base = Url::parse(&format!("{}/", host.trim_end_matches('/')))
        .map_err(|error| CliError::config(format!("Invalid host {host}: {error}")))?;
    let mut url = base
        .join(request.path.trim_start_matches('/'))
        .map_err(|error| CliError::config(format!("Invalid request path: {error}")))?;
    if !request.query.is_empty() {
        url.query_pairs_mut().extend_pairs(&request.query);
    }
    Ok(url)
}

fn headers_json(headers: &HeaderMap) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, value) in headers {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        match out.get_mut(name.as_str()) {
            Some(Value::String(existing)) => {
                existing.push_str(", ");
                existing.push_str(&value);
            }
            _ => {
                out.insert(name.as_str().to_owned(), Value::String(value));
            }
        }
    }
    out
}

/// Send `request` and return the response, including non-2xx ones (exit 5 is
/// decided by the caller). Transport failures are exit 4, invalid JSON in a
/// 2xx response exit 7.
pub async fn execute(
    http: &HttpClient,
    settings: &Settings,
    request: &PreparedRequest,
) -> Result<ApiResult> {
    let captured: Arc<Mutex<Option<(u16, HeaderMap)>>> = Arc::default();
    let sink = Arc::clone(&captured);
    let mut options = RequestOptions::default().with_response_hook(move |info| {
        if let Ok(mut slot) = sink.lock() {
            *slot = Some((info.status.as_u16(), info.headers.clone()));
        }
    });
    for (name, value) in request.meta.headers {
        options = options.with_header(name, value);
    }
    let meta = request.meta;
    let query = &request.query;
    // GET and DELETE-with-query take the query as typed parameters, which keep
    // repeated names; the other methods (none of which repeat a name) take
    // them through the view.
    let typed_query = meta.method == HttpMethod::Get
        || (meta.method == HttpMethod::Delete
            && meta.response == ResponseKind::Empty
            && request.body.is_none());
    if !typed_query {
        for (name, value) in query {
            options = options.with_query_param(name, value);
        }
    }
    let view = http.with_options(&options);
    let path = request.path.as_str();
    let body = request.body.as_ref();
    let empty = Value::Object(Map::new());

    let outcome: std::result::Result<Option<Value>, LangfuseError> =
        match (meta.method, meta.response) {
            (HttpMethod::Get, _) => view.get_with_query::<Value, _>(path, query).await.map(Some),
            (HttpMethod::Post, ResponseKind::Json) => view
                .post::<Value, _>(path, body.unwrap_or(&empty))
                .await
                .map(Some),
            (HttpMethod::Post, ResponseKind::Empty) => view
                .post_no_response(path, body.unwrap_or(&empty))
                .await
                .map(|()| None),
            (HttpMethod::Put, ResponseKind::Json) => view
                .put::<Value, _>(path, body.unwrap_or(&empty))
                .await
                .map(Some),
            (HttpMethod::Patch, ResponseKind::Json) => view
                .patch::<Value, _>(path, body.unwrap_or(&empty))
                .await
                .map(Some),
            (HttpMethod::Patch, ResponseKind::Empty) => view
                .patch_no_content(path, body.unwrap_or(&empty))
                .await
                .map(|()| None),
            (HttpMethod::Put, ResponseKind::Empty) => {
                return Err(CliError::new(
                    Exit::Internal,
                    format!(
                        "{}: PUT without a response body is unsupported",
                        meta.operation_id
                    ),
                ));
            }
            (HttpMethod::Delete, ResponseKind::Json) => match body {
                Some(body) => view
                    .delete_with_body_response::<Value, _>(path, body)
                    .await
                    .map(Some),
                None => view.delete_with_response::<Value>(path).await.map(Some),
            },
            (HttpMethod::Delete, ResponseKind::Empty) => match body {
                Some(body) => view.delete_with_body(path, body).await.map(|()| None),
                None => view
                    .delete_no_content_with_query(path, query)
                    .await
                    .map(|()| None),
            },
        };

    let (status, headers) = captured
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .map(|(status, headers)| (status, headers_json(&headers)))
        .unwrap_or_default();

    match outcome {
        Ok(body) => Ok(ApiResult {
            status: if status == 0 { 200 } else { status },
            headers,
            ok: true,
            body,
        }),
        Err(LangfuseError::Transport(error)) => {
            let url = request_url(&settings.host, request)
                .map(|url| url.to_string())
                .unwrap_or_else(|_| request.path.clone());
            Err(CliError::new(
                Exit::Network,
                format!(
                    "{} {url} failed: {}",
                    meta.method.as_str(),
                    error_chain(&error)
                ),
            ))
        }
        Err(error) => match error.status() {
            Some(error_status) => Ok(ApiResult {
                status: error_status,
                headers,
                ok: false,
                body: error.body().cloned().or_else(|| {
                    error
                        .message()
                        .map(|message| Value::String(message.to_owned()))
                }),
            }),
            None => Err(error.into()),
        },
    }
}

fn shell_quote(value: &str) -> String {
    let safe = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:@%+=,-".contains(&byte));
    if safe {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r#"'"'"'"#))
    }
}

/// The request as a `curl` command (credentials redacted unless `show_secrets`).
pub fn render_curl(
    settings: &Settings,
    request: &PreparedRequest,
    show_secrets: bool,
) -> Result<String> {
    let url = request_url(&settings.host, request)?;
    let public_key = settings.public_key.clone().unwrap_or_default();
    let mut headers: Vec<(String, String)> = vec![
        ("accept".into(), "application/json".into()),
        ("user-agent".into(), USER_AGENT.into()),
        ("x-langfuse-sdk-name".into(), "rust".into()),
        ("x-langfuse-sdk-version".into(), SDK_VERSION.into()),
    ];
    if !public_key.is_empty() {
        headers.push(("x-langfuse-public-key".into(), public_key.clone()));
    }
    if request.meta.auth {
        let value = if show_secrets {
            let credentials = format!(
                "{public_key}:{}",
                settings.secret_key.as_deref().unwrap_or_default()
            );
            format!("Basic {}", STANDARD.encode(credentials))
        } else {
            "Basic <redacted>".to_owned()
        };
        headers.push(("authorization".into(), value));
    }
    if request.body.is_some() {
        headers.push(("content-type".into(), "application/json".into()));
    }
    for (name, value) in request.meta.headers {
        headers.retain(|(existing, _)| existing != name);
        headers.push(((*name).to_owned(), (*value).to_owned()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let mut parts = vec![
        "curl".to_owned(),
        "--request".to_owned(),
        request.meta.method.as_str().to_owned(),
        shell_quote(url.as_str()),
    ];
    for (name, value) in headers {
        parts.push("--header".to_owned());
        parts.push(shell_quote(&format!("{name}: {value}")));
    }
    if let Some(body) = &request.body {
        parts.push("--data".to_owned());
        parts.push(shell_quote(&body.to_string()));
    }
    Ok(parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::OperationMeta;
    use std::time::Duration;

    const META: OperationMeta = OperationMeta::new(
        "prompts",
        "create",
        "prompts_create",
        HttpMethod::Post,
        "/api/public/v2/prompts",
    );

    fn settings(host: &str) -> Settings {
        Settings {
            public_key: Some("pk-lf-1".into()),
            secret_key: Some("sk-lf-1".into()),
            host: host.into(),
            timeout: Duration::from_secs(1),
            max_retries: 0,
        }
    }

    #[test]
    fn url_keeps_a_base_path_prefix() {
        let mut request = PreparedRequest::new(&META);
        request.query.push(("name".into(), "a b".into()));
        let url = request_url("https://example.com/langfuse", &request).unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/langfuse/api/public/v2/prompts?name=a+b"
        );
    }

    #[test]
    fn curl_is_redacted_sorted_and_quoted() {
        let request = PreparedRequest::new(&META).body(serde_json::json!({"name": "it's"}));
        let curl = render_curl(&settings("https://cloud.langfuse.com"), &request, false).unwrap();
        assert_eq!(
            curl,
            format!(
                "curl --request POST https://cloud.langfuse.com/api/public/v2/prompts \
                 --header 'accept: application/json' --header 'authorization: Basic <redacted>' \
                 --header 'content-type: application/json' --header 'user-agent: {USER_AGENT}' \
                 --header 'x-langfuse-public-key: pk-lf-1' --header 'x-langfuse-sdk-name: rust' \
                 --header 'x-langfuse-sdk-version: {SDK_VERSION}' \
                 --data '{{\"name\":\"it'\"'\"'s\"}}'"
            )
        );
        let shown = render_curl(&settings("https://cloud.langfuse.com"), &request, true).unwrap();
        assert!(shown.contains(&format!(
            "authorization: Basic {}",
            STANDARD.encode("pk-lf-1:sk-lf-1")
        )));
    }

    #[test]
    fn sdk_version_matches_the_lockfile() {
        let lock = include_str!("../Cargo.lock");
        let entry = lock
            .split("[[package]]")
            .find(|entry| entry.contains("name = \"langfuse-rust-sdk\"\n"))
            .expect("langfuse-rust-sdk in Cargo.lock");
        assert!(
            entry.contains(&format!("version = \"{SDK_VERSION}\"")),
            "SDK_VERSION is out of date: {entry}"
        );
    }
}
