//! Model calls through the [Langfuse AI Gateway](https://langfuse.com/docs/gateway/overview)
//! (beta).
//!
//! The gateway sits between an application and its model providers. It serves
//! the providers' own APIs (OpenAI Responses and Chat Completions under
//! `{host}/openai/v1`, Anthropic Messages under `{host}/anthropic/v1`),
//! authenticates with a gateway key instead of a provider key, and records
//! every call as a Langfuse generation: model, usage, cost and latency, plus
//! input and output in the `FULL` ingestion mode. Request and response bodies
//! are the provider's; the gateway does not translate between formats.
//!
//! This module provides:
//!
//! - [`GatewayClient`]: the gateway routes as JSON calls (bodies in the
//!   provider's format, as `serde_json::Value` or your own types), streams of
//!   server-sent events ([`GatewayStream`]), retries, and the response
//!   headers of the gateway ([`GatewayResponseInfo`]: the trace and
//!   observation IDs of the recorded generation).
//! - [`GatewayHeaders`]: the optional request headers that enrich the
//!   recorded trace (user, session, environment, tags, metadata, trace name)
//!   and the W3C `traceparent`, encoded the way the gateway reads them.
//! - With the `otel` feature, the trace context: a call made inside an
//!   observation sends its `traceparent` and its propagated attributes (see
//!   `GatewayHeaders::from_context`), so the gateway's generation becomes a
//!   child of that observation, in the same trace as the SDK's spans.
//! - With the `openai` feature, `GatewayOpenAIConfig`: an `async_openai`
//!   configuration pointing at the gateway that adds those headers to every
//!   request.
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::gateway::{GatewayClient, GatewayHeaders};
//! use serde_json::{Value, json};
//!
//! # async fn example() -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! // LANGFUSE_GATEWAY_HOST (e.g. https://gateway.cloud.langfuse.com) and LANGFUSE_GATEWAY_KEY
//! let gateway = GatewayClient::from_env()?;
//!
//! let reply = gateway
//!     .with_headers(GatewayHeaders::default().with_session_id("chat-42").with_tag("support"))
//!     .messages::<Value, _>(&json!({
//!         "model": "anthropic-prod:claude-sonnet-5-5",
//!         "max_tokens": 256,
//!         "messages": [{"role": "user", "content": "Say hello"}],
//!     }))
//!     .await?;
//! println!("{}", reply.body["content"][0]["text"]);
//! println!("recorded as trace {:?}", reply.info.trace_id);
//! # Ok(())
//! # }
//! ```
//!
//! # Model IDs
//!
//! Model IDs have the form `<credential>:<model>`, where `<credential>` is the
//! name of a provider credential of the organization; list them with
//! [`GatewayClient::openai_models`] and [`GatewayClient::anthropic_models`].
//!
//! # Record each call once
//!
//! The gateway already records every call. Do not also trace the same call
//! with `openai::OpenAI` (feature `openai`) or wrap it in a generation of your
//! own: it would be recorded twice. Wrap it in a span (or an agent, chain,
//! ...) instead: with the `otel` feature, the gateway's generation becomes its
//! child.
//!
//! # Projects
//!
//! The gateway records into the ingestion project chosen by the
//! organization's gateway admins ([`GatewayResponseInfo::project_id`]). The
//! SDK's spans and the gateway's generation share one trace only if the
//! tracer exports to that same project, i.e. its API keys belong to it.
//!
//! # Retries and timeouts
//!
//! Model calls are `POST` requests: following the SDK's [`RetryPolicy`], they
//! are retried on 408, 429, 503 and connection failures only, never after the
//! provider may have answered (502 and 504 from the gateway, timeouts), so a
//! retry does not record or bill a call twice. The default timeout is 10
//! minutes per attempt (the one of the OpenAI and Anthropic SDKs), streams
//! included.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use futures_util::Stream;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, USER_AGENT,
};
use reqwest::{Client, Method};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use url::Url;

use crate::error::{LangfuseError, map_response_error};
use crate::http::{parse_retry_after_header, retry_transport_error};
use crate::retry::RetryPolicy;

/// Environment variable holding the gateway host (read by [`GatewayClient::from_env`]).
pub const ENV_GATEWAY_HOST: &str = "LANGFUSE_GATEWAY_HOST";
/// Environment variable holding the gateway key (required by [`GatewayClient::from_env`]).
pub const ENV_GATEWAY_KEY: &str = "LANGFUSE_GATEWAY_KEY";

/// Default timeout of each attempt (10 minutes, as in the OpenAI and
/// Anthropic SDKs): model calls, streams included, can take minutes.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Connect timeout of the SDK-built client, so an unreachable host fails fast.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// `anthropic-version` sent on the Anthropic routes unless set otherwise.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The gateway caps each context header value at 1 KiB.
const MAX_VALUE_BYTES: usize = 1024;
/// The gateway ignores all context headers when they add up to more than 8 KiB.
const MAX_CONTEXT_BYTES: usize = 8 * 1024;
/// Environment names are at most 40 characters.
const MAX_ENVIRONMENT_CHARS: usize = 40;

/// Gateway hosts of Langfuse Cloud, one per region. A gateway key only works
/// on the host of its own region; self-hosted deployments use the URL they
/// configured (`LANGFUSE_AI_GATEWAY_PUBLIC_URL`).
pub mod hosts {
    /// EU region (`cloud.langfuse.com`).
    pub const EU: &str = "https://gateway.cloud.langfuse.com";
    /// US region (`us.cloud.langfuse.com`).
    pub const US: &str = "https://gateway.us.cloud.langfuse.com";
    /// Japan region (`jp.cloud.langfuse.com`).
    pub const JP: &str = "https://gateway.jp.cloud.langfuse.com";
    /// HIPAA region (`hipaa.cloud.langfuse.com`).
    pub const HIPAA: &str = "https://gateway.hipaa.cloud.langfuse.com";

    /// The gateway host of the Langfuse Cloud region of `base_url` (the
    /// API base URL of the client, e.g. `https://us.cloud.langfuse.com`), or
    /// `None` for other hosts (self-hosted deployments).
    ///
    /// ```
    /// use langfuse_rust_sdk::gateway::hosts;
    ///
    /// assert_eq!(hosts::for_base_url("https://us.cloud.langfuse.com"), Some(hosts::US));
    /// assert_eq!(hosts::for_base_url("https://cloud.langfuse.com/"), Some(hosts::EU));
    /// assert_eq!(hosts::for_base_url("https://langfuse.example.com"), None);
    /// ```
    pub fn for_base_url(base_url: &str) -> Option<&'static str> {
        let url = url::Url::parse(base_url.trim()).ok()?;
        match url.host_str()? {
            "cloud.langfuse.com" | "eu.cloud.langfuse.com" => Some(EU),
            "us.cloud.langfuse.com" => Some(US),
            "jp.cloud.langfuse.com" => Some(JP),
            "hipaa.cloud.langfuse.com" => Some(HIPAA),
            _ => None,
        }
    }
}

/// Request header names read by the gateway.
mod names {
    pub(super) const USER_ID: &str = "langfuse-user-id";
    pub(super) const SESSION_ID: &str = "langfuse-session-id";
    pub(super) const ENVIRONMENT: &str = "langfuse-environment";
    pub(super) const TAGS: &str = "langfuse-tags";
    pub(super) const METADATA: &str = "langfuse-metadata";
    pub(super) const TRACE_NAME: &str = "langfuse-trace-name";
    pub(super) const TRACEPARENT: &str = "traceparent";
    pub(super) const TRACESTATE: &str = "tracestate";
}

/// Bytes percent-encoded in the `langfuse-*` header values (the gateway
/// percent-decodes them): controls, space, `%` and the list separator `,`.
/// Non-ASCII characters are always encoded.
const VALUE: &AsciiSet = &CONTROLS.add(b' ').add(b'%').add(b',');
/// Metadata keys also encode `:`, which separates a key from its value.
const METADATA_KEY: &AsciiSet = &VALUE.add(b':');

fn encode(value: &str, set: &'static AsciiSet) -> String {
    utf8_percent_encode(value, set).to_string()
}

/// Optional request headers that enrich the trace the gateway records, plus
/// the W3C trace context that places its generation in an existing trace.
/// See [request headers](https://langfuse.com/docs/gateway/features/tracing#request-headers).
///
/// Build it from [`GatewayHeaders::default`] with the `with_*` methods and
/// pass it to [`GatewayClient::with_headers`] (or
/// [`GatewayClientBuilder::headers`] for every call), or turn it into
/// headers for another HTTP client with [`to_header_map`](Self::to_header_map).
///
/// ```
/// use langfuse_rust_sdk::gateway::GatewayHeaders;
///
/// let headers = GatewayHeaders::default()
///     .with_session_id("chat-42")
///     .with_environment("production")
///     .with_tags(["checkout", "beta, internal"])
///     .with_metadata("repo", "shop:web");
/// let map = headers.to_header_map();
/// assert_eq!(map["langfuse-tags"], "checkout,beta%2C%20internal");
/// assert_eq!(map["langfuse-metadata"], "repo:shop:web");
/// ```
///
/// Values are percent-encoded (`%`, `,`, spaces, controls and non-ASCII
/// characters; `:` too in metadata keys), as the gateway decodes them.
/// Empty values and invalid environments are left out, values over the
/// gateway's 1 KiB limit are dropped (tags and metadata entries one by one,
/// from the last), and metadata then tags are dropped when all the headers
/// would exceed the 8 KiB after which the gateway ignores every one of them,
/// `traceparent` included; each drop logs a warning.
///
/// The gateway only uses `user_id` with service account keys: user keys
/// always record the key owner. Do not set a fixed `session_id` or
/// `trace_name` for every call of a long-lived client: it would merge all
/// sessions into one, or rename every trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GatewayHeaders {
    /// Trace user (`langfuse-user-id`); service account keys only.
    pub user_id: Option<String>,
    /// Session (`langfuse-session-id`), replacing the one the gateway infers
    /// from coding agents.
    pub session_id: Option<String>,
    /// Environment (`langfuse-environment`): at most 40 lowercase letters,
    /// digits, `-` or `_`, not starting with `langfuse`.
    pub environment: Option<String>,
    /// Tags (`langfuse-tags`).
    pub tags: Vec<String>,
    /// Top-level metadata (`langfuse-metadata`); values stay strings.
    pub metadata: BTreeMap<String, String>,
    /// Trace name (`langfuse-trace-name`).
    pub trace_name: Option<String>,
    /// W3C `traceparent` (`00-<trace id>-<parent span id>-<flags>`): the
    /// gateway records its generation in that trace, under that span.
    pub traceparent: Option<String>,
    /// W3C `tracestate`, kept by the gateway.
    pub tracestate: Option<String>,
}

impl GatewayHeaders {
    /// Set [`user_id`](Self::user_id).
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set [`session_id`](Self::session_id).
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set [`environment`](Self::environment).
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Replace [`tags`](Self::tags).
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Add one tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set `metadata[key]` to `value`.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set [`trace_name`](Self::trace_name).
    pub fn with_trace_name(mut self, trace_name: impl Into<String>) -> Self {
        self.trace_name = Some(trace_name.into());
        self
    }

    /// Set [`traceparent`](Self::traceparent).
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Set [`tracestate`](Self::tracestate).
    pub fn with_tracestate(mut self, tracestate: impl Into<String>) -> Self {
        self.tracestate = Some(tracestate.into());
        self
    }

    /// `self` with the fields set in `top` taking precedence: single values
    /// of `top` replace those of `self`, tags are joined (without
    /// duplicates) and metadata merged (keys of `top` win).
    pub fn overlay(&self, top: &GatewayHeaders) -> GatewayHeaders {
        fn pick(top: &Option<String>, base: &Option<String>) -> Option<String> {
            top.clone().or_else(|| base.clone())
        }
        let mut tags = self.tags.clone();
        for tag in &top.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        let mut metadata = self.metadata.clone();
        metadata.extend(top.metadata.iter().map(|(k, v)| (k.clone(), v.clone())));
        GatewayHeaders {
            user_id: pick(&top.user_id, &self.user_id),
            session_id: pick(&top.session_id, &self.session_id),
            environment: pick(&top.environment, &self.environment),
            tags,
            metadata,
            trace_name: pick(&top.trace_name, &self.trace_name),
            traceparent: pick(&top.traceparent, &self.traceparent),
            tracestate: pick(&top.tracestate, &self.tracestate),
        }
    }

    /// The trace context and propagated attributes of `cx` (feature `otel`):
    ///
    /// - `traceparent` (and `tracestate`) of the current span of `cx` (a
    ///   Langfuse observation or any other OpenTelemetry span), so the
    ///   gateway's generation becomes its child;
    /// - the attributes set with
    ///   [`propagate_attributes`](crate::otel::propagate_attributes) or
    ///   received as `langfuse_*` baggage: user, session, environment, tags,
    ///   metadata and trace name (version and prompt have no gateway header);
    /// - else the environment of the current observation's tracer, so the
    ///   generation lands in the same environment as the SDK's spans.
    ///
    /// [`GatewayClient`] and `GatewayOpenAIConfig` (feature `openai`) add these headers to
    /// every call made inside an observation; use this function for other
    /// HTTP clients, with [`to_header_map`](Self::to_header_map).
    #[cfg(feature = "otel")]
    pub fn from_context(cx: &opentelemetry::Context) -> GatewayHeaders {
        crate::otel::gateway_context_headers(cx)
    }

    /// [`from_context`](Self::from_context) of the current context (feature `otel`).
    #[cfg(feature = "otel")]
    pub fn current() -> GatewayHeaders {
        Self::from_context(&opentelemetry::Context::current())
    }

    /// The HTTP headers, encoded and within the gateway's limits (see the
    /// [type docs](Self)).
    pub fn to_header_map(&self) -> HeaderMap {
        let mut values: Vec<(&'static str, String)> = Vec::new();
        let scalars = [
            (names::USER_ID, &self.user_id),
            (names::SESSION_ID, &self.session_id),
            (names::TRACE_NAME, &self.trace_name),
        ];
        for (name, value) in scalars {
            if let Some(value) = value.as_deref().filter(|v| !v.is_empty()) {
                push_capped(&mut values, name, encode(value, VALUE));
            }
        }
        if let Some(environment) = self.environment.as_deref().filter(|v| !v.is_empty()) {
            if valid_environment(environment) {
                values.push((names::ENVIRONMENT, environment.to_owned()));
            } else {
                tracing::warn!(
                    environment,
                    "gateway environment must be at most 40 lowercase letters, digits, '-' or '_' \
                     and not start with 'langfuse'; header left out"
                );
            }
        }
        if let Some(traceparent) = self.traceparent.as_deref().filter(|v| !v.is_empty()) {
            if valid_traceparent(traceparent) {
                values.push((names::TRACEPARENT, traceparent.to_owned()));
            } else {
                tracing::warn!(traceparent, "invalid W3C traceparent; header left out");
            }
        }
        if let Some(tracestate) = self.tracestate.as_deref().filter(|v| !v.is_empty()) {
            push_capped(&mut values, names::TRACESTATE, tracestate.to_owned());
        }

        let mut tags: Vec<String> = Vec::new();
        for tag in self.tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
            let tag = encode(tag, VALUE);
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        let mut tags = fit_list(tags, "tags");
        let mut metadata: Vec<String> = self
            .metadata
            .iter()
            .filter(|(key, _)| !key.is_empty())
            .map(|(key, value)| format!("{}:{}", encode(key, METADATA_KEY), encode(value, VALUE)))
            .collect();
        metadata = fit_list(metadata, "metadata");

        // Above 8 KiB the gateway ignores every context header: drop
        // metadata, then tags, so the trace context and the IDs survive.
        let size = |values: &[(&str, String)], lists: &[&Vec<String>]| -> usize {
            values
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>()
                + lists
                    .iter()
                    .zip([names::TAGS, names::METADATA])
                    .filter(|(list, _)| !list.is_empty())
                    .map(|(list, name)| name.len() + joined_len(list))
                    .sum::<usize>()
        };
        let mut dropped = Vec::new();
        while size(&values, &[&tags, &metadata]) > MAX_CONTEXT_BYTES {
            if metadata.pop().is_some() {
                dropped.push("metadata");
            } else if tags.pop().is_some() {
                dropped.push("tags");
            } else {
                break;
            }
        }
        if !dropped.is_empty() {
            tracing::warn!(
                dropped = dropped.len(),
                "gateway context headers exceed 8 KiB; dropped metadata entries and tags"
            );
        }
        if !tags.is_empty() {
            values.push((names::TAGS, tags.join(",")));
        }
        if !metadata.is_empty() {
            values.push((names::METADATA, metadata.join(",")));
        }

        let mut map = HeaderMap::with_capacity(values.len());
        for (name, value) in values {
            match HeaderValue::from_str(&value) {
                Ok(value) => {
                    map.insert(HeaderName::from_static(name), value);
                }
                Err(_) => tracing::warn!(header = name, "invalid header value; left out"),
            }
        }
        map
    }
}

fn push_capped(values: &mut Vec<(&'static str, String)>, name: &'static str, value: String) {
    if value.len() > MAX_VALUE_BYTES {
        tracing::warn!(
            header = name,
            bytes = value.len(),
            "gateway header value is over 1 KiB; left out"
        );
        return;
    }
    values.push((name, value));
}

fn joined_len(items: &[String]) -> usize {
    items.iter().map(String::len).sum::<usize>() + items.len().saturating_sub(1)
}

/// `items` without its last entries until the joined list fits in 1 KiB.
fn fit_list(mut items: Vec<String>, what: &str) -> Vec<String> {
    let before = items.len();
    while !items.is_empty() && joined_len(&items) > MAX_VALUE_BYTES {
        items.pop();
    }
    if items.len() < before {
        tracing::warn!(
            header = what,
            dropped = before - items.len(),
            "gateway header value is over 1 KiB; dropped its last entries"
        );
    }
    items
}

fn valid_environment(value: &str) -> bool {
    value.len() <= MAX_ENVIRONMENT_CHARS
        && !value.starts_with("langfuse")
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// `version-traceid-spanid-flags` in lowercase hex, without all-zero IDs.
fn valid_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    let hex = |s: &str, len: usize| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    let nonzero = |s: &str| s.bytes().any(|b| b != b'0');
    parts.len() == 4
        && hex(parts[0], 2)
        && parts[0] != "ff"
        && hex(parts[1], 32)
        && nonzero(parts[1])
        && hex(parts[2], 16)
        && nonzero(parts[2])
        && hex(parts[3], 2)
}

/// The gateway's response headers.
/// See [response headers](https://langfuse.com/docs/gateway/features/tracing#response-headers).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GatewayResponseInfo {
    /// `langfuse-request-id`: the gateway request ID (quote it when
    /// reporting a problem).
    pub request_id: Option<String>,
    /// `langfuse-trace-id`: trace of the recorded generation (calls that
    /// produce one: responses, chat completions, messages).
    pub trace_id: Option<String>,
    /// `langfuse-observation-id`: ID of the recorded generation; use it
    /// with `trace_id` to score the call.
    pub observation_id: Option<String>,
    /// `langfuse-organization-id`.
    pub organization_id: Option<String>,
    /// `langfuse-project-id`: the project that receives the gateway's traces.
    pub project_id: Option<String>,
    /// `langfuse-provider`: provider that served the call (`openai`, `bedrock`, ...).
    pub provider: Option<String>,
    /// `langfuse-provider-connection-id`: provider credential that served the call.
    pub provider_connection_id: Option<String>,
}

impl GatewayResponseInfo {
    /// Read the gateway's headers from a response (of any HTTP client).
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let get = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        Self {
            request_id: get("langfuse-request-id"),
            trace_id: get("langfuse-trace-id"),
            observation_id: get("langfuse-observation-id"),
            organization_id: get("langfuse-organization-id"),
            project_id: get("langfuse-project-id"),
            provider: get("langfuse-provider"),
            provider_connection_id: get("langfuse-provider-connection-id"),
        }
    }
}

/// A JSON response of the gateway: the provider's body and the gateway's headers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GatewayResponse<T> {
    /// The response body, in the provider's format.
    pub body: T,
    /// The gateway's response headers.
    pub info: GatewayResponseInfo,
}

/// Builder of a [`GatewayClient`]; start with [`GatewayClient::builder`] or
/// [`GatewayClientBuilder::from_env`].
pub struct GatewayClientBuilder {
    host: String,
    key: String,
    timeout: Option<Duration>,
    retry_policy: RetryPolicy,
    http_client: Option<Client>,
    headers: GatewayHeaders,
    extra_headers: HeaderMap,
    header_error: Option<String>,
    propagate_trace_context: bool,
}

impl fmt::Debug for GatewayClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayClientBuilder")
            .field("host", &self.host)
            .field("key", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("retry_policy", &self.retry_policy)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

impl GatewayClientBuilder {
    /// A builder from the environment: the key from `LANGFUSE_GATEWAY_KEY`
    /// (required) and the host from `LANGFUSE_GATEWAY_HOST`, else the
    /// gateway of the Langfuse Cloud region of `LANGFUSE_BASE_URL` (or
    /// `LANGFUSE_HOST`), see [`hosts::for_base_url`].
    pub fn from_env() -> Result<Self, LangfuseError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, LangfuseError> {
        let get = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let key = get(ENV_GATEWAY_KEY).ok_or_else(|| {
            LangfuseError::Config(format!(
                "{ENV_GATEWAY_KEY} is not set: create a gateway key under AI Gateway -> Gateway keys"
            ))
        })?;
        let host = match get(ENV_GATEWAY_HOST) {
            Some(host) => host,
            None => get(crate::config::ENV_BASE_URL)
                .or_else(|| get(crate::config::ENV_HOST))
                .and_then(|base| hosts::for_base_url(&base))
                .map(str::to_owned)
                .ok_or_else(|| {
                    LangfuseError::Config(format!(
                        "{ENV_GATEWAY_HOST} is not set (e.g. {}) and LANGFUSE_BASE_URL is not a \
                         Langfuse Cloud region",
                        hosts::EU
                    ))
                })?,
        };
        Ok(GatewayClient::builder(host, key))
    }

    /// Timeout of each attempt (default: [`DEFAULT_TIMEOUT`]). With a custom
    /// [`http_client`](Self::http_client), it is applied per request.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Retry policy (default: [`RetryPolicy::default`]); see the
    /// [module docs](self#retries-and-timeouts) for what is retried.
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Use your own `reqwest::Client` (proxies, TLS settings, ...).
    pub fn http_client(mut self, client: Client) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Gateway headers sent with every call (calls can add more with
    /// [`GatewayClient::with_headers`]).
    pub fn headers(mut self, headers: GatewayHeaders) -> Self {
        self.headers = headers;
        self
    }

    /// Another header sent with every call (e.g. `anthropic-beta`); an
    /// invalid header makes [`build`](Self::build) fail.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        match parse_header(name, value) {
            Ok((name, value)) => {
                self.extra_headers.insert(name, value);
            }
            Err(e) => {
                self.header_error.get_or_insert(e);
            }
        }
        self
    }

    /// Whether calls send the trace context of the current observation
    /// (feature `otel`, default `true`); see `GatewayHeaders::from_context`.
    pub fn propagate_trace_context(mut self, propagate: bool) -> Self {
        self.propagate_trace_context = propagate;
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<GatewayClient, LangfuseError> {
        if let Some(message) = self.header_error {
            return Err(LangfuseError::Config(message));
        }
        let host = parse_host(&self.host)?;
        let key = self.key.trim();
        if key.is_empty() {
            return Err(LangfuseError::Config("the gateway key is empty".into()));
        }
        let mut auth = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| LangfuseError::Config("the gateway key is not a valid header".into()))?;
        auth.set_sensitive(true);
        let (client, request_timeout) = match self.http_client {
            Some(client) => (client, self.timeout),
            None => {
                let timeout = self.timeout.unwrap_or(DEFAULT_TIMEOUT);
                let client = Client::builder()
                    .timeout(timeout)
                    .connect_timeout(CONNECT_TIMEOUT.min(timeout))
                    .build()
                    .map_err(LangfuseError::Transport)?;
                (client, None)
            }
        };
        Ok(GatewayClient {
            shared: Arc::new(Shared {
                client,
                host,
                key: key.to_owned(),
                auth,
                retry_policy: self.retry_policy,
                request_timeout,
            }),
            headers: self.headers,
            extra_headers: self.extra_headers,
            propagate_trace_context: self.propagate_trace_context,
        })
    }
}

fn parse_header(name: &str, value: &str) -> Result<(HeaderName, HeaderValue), String> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| format!("invalid header name: {name}"))?;
    let value =
        HeaderValue::from_str(value).map_err(|_| format!("invalid value for header {name}"))?;
    Ok((name, value))
}

/// The host as `scheme://host[:port][/path]`, without a trailing `/`.
fn parse_host(host: &str) -> Result<String, LangfuseError> {
    let url = Url::parse(host.trim())
        .map_err(|e| LangfuseError::Config(format!("invalid gateway host {host:?}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(LangfuseError::Config(format!(
            "invalid gateway host {host:?}: expected an http(s) URL such as {}",
            hosts::EU
        )));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(LangfuseError::Config(format!(
            "invalid gateway host {host:?}: no query or fragment allowed"
        )));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Client of the [Langfuse AI Gateway](self): see the [module docs](self).
///
/// Clones share the connection pool; [`with_headers`](Self::with_headers)
/// and [`with_header`](Self::with_header) return such clones with more
/// headers, for one call or a group of calls.
#[derive(Clone)]
pub struct GatewayClient {
    shared: Arc<Shared>,
    headers: GatewayHeaders,
    extra_headers: HeaderMap,
    propagate_trace_context: bool,
}

struct Shared {
    client: Client,
    /// Without a trailing `/`.
    host: String,
    key: String,
    auth: HeaderValue,
    retry_policy: RetryPolicy,
    /// Per-request timeout, set only for a custom client with an explicit timeout.
    request_timeout: Option<Duration>,
}

impl fmt::Debug for GatewayClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayClient")
            .field("host", &self.shared.host)
            .field("key", &"<redacted>")
            .field("headers", &self.headers)
            .field("propagate_trace_context", &self.propagate_trace_context)
            .finish_non_exhaustive()
    }
}

impl GatewayClient {
    /// A builder for the gateway at `host` (e.g. [`hosts::EU`]) with a gateway key.
    pub fn builder(host: impl Into<String>, key: impl Into<String>) -> GatewayClientBuilder {
        GatewayClientBuilder {
            host: host.into(),
            key: key.into(),
            timeout: None,
            retry_policy: RetryPolicy::default(),
            http_client: None,
            headers: GatewayHeaders::default(),
            extra_headers: HeaderMap::new(),
            header_error: None,
            propagate_trace_context: true,
        }
    }

    /// A client for the gateway at `host` with the default settings.
    pub fn new(host: impl Into<String>, key: impl Into<String>) -> Result<Self, LangfuseError> {
        Self::builder(host, key).build()
    }

    /// A client from the environment; see [`GatewayClientBuilder::from_env`].
    pub fn from_env() -> Result<Self, LangfuseError> {
        GatewayClientBuilder::from_env()?.build()
    }

    /// The gateway host, without a trailing `/`.
    pub fn host(&self) -> &str {
        &self.shared.host
    }

    /// Base URL of the OpenAI route (`{host}/openai/v1`), for OpenAI clients.
    pub fn openai_base_url(&self) -> String {
        format!("{}/openai/v1", self.shared.host)
    }

    /// Base URL of the Anthropic route without `/v1` (`{host}/anthropic`),
    /// for Anthropic clients, which append `/v1/messages`.
    pub fn anthropic_base_url(&self) -> String {
        format!("{}/anthropic", self.shared.host)
    }

    /// The gateway key, to configure other clients (it is sent as
    /// `Authorization: Bearer`).
    pub fn key(&self) -> &str {
        &self.shared.key
    }

    /// A clone that also sends `headers` (its fields replace those already
    /// set; tags are added and metadata merged, see [`GatewayHeaders::overlay`]).
    pub fn with_headers(&self, headers: GatewayHeaders) -> GatewayClient {
        GatewayClient {
            headers: self.headers.overlay(&headers),
            ..self.clone()
        }
    }

    /// A clone that also sends the header `name` (e.g. `anthropic-beta`).
    pub fn with_header(&self, name: &str, value: &str) -> Result<GatewayClient, LangfuseError> {
        let (name, value) = parse_header(name, value).map_err(LangfuseError::Config)?;
        let mut client = self.clone();
        client.extra_headers.insert(name, value);
        Ok(client)
    }

    /// A clone that sends (or not) the trace context of the current
    /// observation (feature `otel`); see `GatewayHeaders::from_context`.
    pub fn with_trace_context_propagation(&self, propagate: bool) -> GatewayClient {
        GatewayClient {
            propagate_trace_context: propagate,
            ..self.clone()
        }
    }

    /// The gateway headers of a call made now: the trace context of the
    /// current observation (feature `otel`, unless disabled) overlaid with
    /// the headers of this client.
    pub fn gateway_headers(&self) -> GatewayHeaders {
        effective_headers(&self.headers, self.propagate_trace_context)
    }

    /// `GET /openai/v1/models`: the models the key can call on the OpenAI route.
    pub async fn openai_models<T: DeserializeOwned>(
        &self,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.get("/openai/v1/models").await
    }

    /// `GET /anthropic/v1/models?limit=1000`: the models the key can call on
    /// the Anthropic route.
    pub async fn anthropic_models<T: DeserializeOwned>(
        &self,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.get("/anthropic/v1/models?limit=1000").await
    }

    /// `POST /openai/v1/responses` (OpenAI Responses API).
    pub async fn responses<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.post("/openai/v1/responses", body).await
    }

    /// `POST /openai/v1/responses` with `"stream": true`.
    pub async fn responses_stream<B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayStream, LangfuseError> {
        self.post_stream("/openai/v1/responses", &streaming(body)?)
            .await
    }

    /// `POST /openai/v1/responses/compact` (compact a response).
    pub async fn responses_compact<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.post("/openai/v1/responses/compact", body).await
    }

    /// `POST /openai/v1/chat/completions` (OpenAI Chat Completions API).
    pub async fn chat_completions<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.post("/openai/v1/chat/completions", body).await
    }

    /// `POST /openai/v1/chat/completions` with `"stream": true`.
    ///
    /// The gateway sets `stream_options.include_usage`, so the last chunk
    /// before `data: [DONE]` has an empty `choices` array and the usage.
    pub async fn chat_completions_stream<B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayStream, LangfuseError> {
        self.post_stream("/openai/v1/chat/completions", &streaming(body)?)
            .await
    }

    /// `POST /anthropic/v1/messages` (Anthropic Messages API).
    pub async fn messages<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.post("/anthropic/v1/messages", body).await
    }

    /// `POST /anthropic/v1/messages` with `"stream": true`.
    pub async fn messages_stream<B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayStream, LangfuseError> {
        self.post_stream("/anthropic/v1/messages", &streaming(body)?)
            .await
    }

    /// `POST /anthropic/v1/messages/count_tokens`.
    pub async fn count_tokens<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        self.post("/anthropic/v1/messages/count_tokens", body).await
    }

    /// `GET {host}{path}` (e.g. a route added to the gateway after this release).
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        let (response, info) = self.send(Method::GET, path, None, false).await?;
        json_response(response, info).await
    }

    /// `POST {host}{path}` with a JSON body.
    pub async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<GatewayResponse<T>, LangfuseError> {
        let body = serde_json::to_vec(body)?;
        let (response, info) = self.send(Method::POST, path, Some(body), false).await?;
        json_response(response, info).await
    }

    /// `POST {host}{path}` with a JSON body, reading the response as
    /// server-sent events (the body is sent unchanged: set `"stream": true`
    /// yourself).
    pub async fn post_stream<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<GatewayStream, LangfuseError> {
        let body = serde_json::to_vec(body)?;
        let (response, info) = self.send(Method::POST, path, Some(body), true).await?;
        Ok(GatewayStream::new(response, info))
    }

    /// `async-openai` configuration for this gateway (feature `openai`); see
    /// [`GatewayOpenAIConfig`].
    #[cfg(feature = "openai")]
    pub fn openai_config(&self) -> GatewayOpenAIConfig {
        GatewayOpenAIConfig {
            inner: async_openai::config::OpenAIConfig::new()
                .with_api_base(self.openai_base_url())
                .with_api_key(self.shared.key.clone())
                // Not the OpenAI organization and project of the environment.
                .with_org_id("")
                .with_project_id(""),
            headers: self.headers.clone(),
            extra_headers: self.extra_headers.clone(),
            propagate_trace_context: self.propagate_trace_context,
        }
    }

    /// Headers of one request: authentication, defaults, gateway headers
    /// (computed now, in the caller's context) and extra headers.
    fn request_headers(&self, path: &str, stream: bool) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, self.shared.auth.clone());
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("langfuse-rust-sdk/", env!("CARGO_PKG_VERSION"))),
        );
        if path.starts_with("/anthropic/") {
            headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static(ANTHROPIC_VERSION),
            );
        }
        if stream {
            headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        }
        headers.extend(self.gateway_headers().to_header_map());
        for (name, value) in &self.extra_headers {
            headers.insert(name, value.clone());
        }
        headers
    }

    /// Send a request with retries; returns the 2xx response with its body unread.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        stream: bool,
    ) -> Result<(reqwest::Response, GatewayResponseInfo), LangfuseError> {
        // Before the first await, so the trace context is the caller's.
        let mut headers = self.request_headers(path, stream);
        if body.is_some() && !headers.contains_key(CONTENT_TYPE) {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        let shared = &*self.shared;
        let path = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        let url = Url::parse(&format!("{}{path}", shared.host)).map_err(LangfuseError::UrlParse)?;
        let idempotent = method == Method::GET;
        let max_retries = shared.retry_policy.max_retries;
        let mut attempt = 0;
        loop {
            tracing::debug!(method = %method, url = %url, attempt, "sending gateway request");
            let mut builder = shared
                .client
                .request(method.clone(), url.clone())
                .headers(headers.clone());
            if let Some(timeout) = shared.request_timeout {
                builder = builder.timeout(timeout);
            }
            if let Some(body) = &body {
                builder = builder.body(body.clone());
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(err) if attempt < max_retries && retry_transport_error(&err, idempotent) => {
                    let delay = shared.retry_policy.delay(attempt, None);
                    tracing::debug!(attempt, delay_ms = delay.as_millis(), error = %err, "retrying");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Err(err) => return Err(LangfuseError::Transport(err)),
            };
            let status = response.status();
            if status.is_success() {
                let info = GatewayResponseInfo::from_headers(response.headers());
                return Ok((response, info));
            }
            let response_headers = response.headers().clone();
            let bytes = response.bytes().await.unwrap_or_default();
            if attempt >= max_retries || !RetryPolicy::is_retryable_for(status, idempotent) {
                let request_id = response_headers
                    .get("langfuse-request-id")
                    .and_then(|v| v.to_str().ok());
                tracing::debug!(status = %status, request_id, "gateway request failed");
                return Err(map_response_error(status, &bytes, &response_headers));
            }
            let delay = shared
                .retry_policy
                .delay(attempt, parse_retry_after_header(&response_headers));
            tracing::debug!(attempt, delay_ms = delay.as_millis(), status = %status, "retrying");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }
}

/// The trace context of the current context (when enabled and with the
/// `otel` feature) overlaid with `headers`.
fn effective_headers(headers: &GatewayHeaders, propagate: bool) -> GatewayHeaders {
    #[cfg(feature = "otel")]
    if propagate {
        return GatewayHeaders::current().overlay(headers);
    }
    let _ = propagate;
    headers.clone()
}

/// `body` as a JSON object with `"stream": true`.
fn streaming<B: Serialize + ?Sized>(body: &B) -> Result<Value, LangfuseError> {
    match serde_json::to_value(body)? {
        Value::Object(mut object) => {
            object.insert("stream".into(), Value::Bool(true));
            Ok(Value::Object(object))
        }
        _ => Err(LangfuseError::Config(
            "the request body must be a JSON object".into(),
        )),
    }
}

async fn json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    info: GatewayResponseInfo,
) -> Result<GatewayResponse<T>, LangfuseError> {
    let bytes = response.bytes().await.map_err(LangfuseError::Transport)?;
    let body = serde_json::from_slice(&bytes)?;
    Ok(GatewayResponse { body, info })
}

/// One server-sent event of a [`GatewayStream`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerSentEvent {
    /// The `event:` field (Anthropic and Responses API streams name their
    /// events; Chat Completions streams do not).
    pub event: Option<String>,
    /// The `data:` lines, joined with `\n`: a JSON document, or `[DONE]` at
    /// the end of a Chat Completions stream.
    pub data: String,
    /// The last `id:` field received, if any.
    pub id: Option<String>,
}

impl ServerSentEvent {
    /// Whether this is the `[DONE]` marker that ends Chat Completions streams.
    pub fn is_done(&self) -> bool {
        self.data == "[DONE]"
    }

    /// Parse [`data`](Self::data) as JSON.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, LangfuseError> {
        Ok(serde_json::from_str(&self.data)?)
    }
}

type EventStream = Pin<Box<dyn Stream<Item = Result<ServerSentEvent, LangfuseError>> + Send>>;

/// The server-sent events of a streamed gateway call, with the gateway's
/// response headers ([`info`](Self::info), available before the first event).
///
/// The events are the provider's: an error in the middle of the stream
/// arrives as an event (for example `event: error` from Anthropic), and a
/// broken connection as an `Err` item, after which the stream ends.
pub struct GatewayStream {
    info: GatewayResponseInfo,
    events: EventStream,
}

impl fmt::Debug for GatewayStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayStream")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl GatewayStream {
    fn new(response: reqwest::Response, info: GatewayResponseInfo) -> Self {
        let state = (Some(response), SseParser::default(), VecDeque::new());
        let events = futures_util::stream::unfold(
            state,
            |(mut response, mut parser, mut queue)| async move {
                loop {
                    if let Some(event) = queue.pop_front() {
                        return Some((Ok(event), (response, parser, queue)));
                    }
                    let body = response.as_mut()?;
                    match body.chunk().await {
                        Ok(Some(bytes)) => parser.push(&bytes, &mut queue),
                        Ok(None) => {
                            parser.finish(&mut queue);
                            response = None;
                        }
                        Err(err) => {
                            return Some((
                                Err(LangfuseError::Transport(err)),
                                (None, parser, queue),
                            ));
                        }
                    }
                }
            },
        );
        Self {
            info,
            events: Box::pin(events),
        }
    }

    /// The gateway's response headers.
    pub fn info(&self) -> &GatewayResponseInfo {
        &self.info
    }
}

impl Stream for GatewayStream {
    type Item = Result<ServerSentEvent, LangfuseError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.events.as_mut().poll_next(cx)
    }
}

/// Incremental parser of a `text/event-stream` body (WHATWG HTML, "server-sent events").
#[derive(Debug, Default)]
struct SseParser {
    line: Vec<u8>,
    /// The previous byte ended a line with `\r`: a following `\n` belongs to it.
    after_cr: bool,
    event: Option<String>,
    data: Option<String>,
    id: Option<String>,
}

impl SseParser {
    fn push(&mut self, bytes: &[u8], out: &mut VecDeque<ServerSentEvent>) {
        for &byte in bytes {
            if std::mem::take(&mut self.after_cr) && byte == b'\n' {
                continue;
            }
            match byte {
                b'\n' => self.end_line(out),
                b'\r' => {
                    self.after_cr = true;
                    self.end_line(out);
                }
                _ => self.line.push(byte),
            }
        }
    }

    /// End of the body: an event without its final blank line is dispatched too.
    fn finish(&mut self, out: &mut VecDeque<ServerSentEvent>) {
        if !self.line.is_empty() {
            self.end_line(out);
        }
        self.dispatch(out);
    }

    fn end_line(&mut self, out: &mut VecDeque<ServerSentEvent>) {
        let line = std::mem::take(&mut self.line);
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        let line = String::from_utf8_lossy(&line);
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_ref(), ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => match &mut self.data {
                Some(data) => {
                    data.push('\n');
                    data.push_str(value);
                }
                None => self.data = Some(value.to_owned()),
            },
            "id" if !value.contains('\0') => self.id = Some(value.to_owned()),
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut VecDeque<ServerSentEvent>) {
        let event = self.event.take();
        if let Some(data) = self.data.take() {
            out.push_back(ServerSentEvent {
                event,
                data,
                id: self.id.clone(),
            });
        }
    }
}

/// `async-openai` configuration for the Langfuse AI Gateway (feature `openai`),
/// from [`GatewayClient::openai_config`].
///
/// It points at `{host}/openai/v1` with the gateway key and adds, to every
/// request, the client's [`GatewayHeaders`] and other headers and (unless
/// disabled) the trace context of the observation current when the request
/// is sent (see [`GatewayHeaders::from_context`]):
///
/// ```rust,no_run
/// use async_openai::types::responses::CreateResponseArgs;
/// use langfuse_rust_sdk::gateway::GatewayClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let gateway = GatewayClient::from_env()?;
/// let openai = async_openai::Client::with_config(gateway.openai_config());
/// let request = CreateResponseArgs::default()
///     .model("openai-prod:gpt-4.1-mini")
///     .input("Say hello")
///     .build()?;
/// let response = openai.responses().create(request).await?;
/// # Ok(())
/// # }
/// ```
///
/// Use the plain `async_openai::Client`, not [`openai::OpenAI`](crate::openai::OpenAI):
/// the gateway already records every call.
#[cfg(feature = "openai")]
#[derive(Clone, Debug)]
pub struct GatewayOpenAIConfig {
    inner: async_openai::config::OpenAIConfig,
    headers: GatewayHeaders,
    extra_headers: HeaderMap,
    propagate_trace_context: bool,
}

#[cfg(feature = "openai")]
impl GatewayOpenAIConfig {
    /// The configuration with `headers` added (see [`GatewayHeaders::overlay`]).
    pub fn with_headers(mut self, headers: GatewayHeaders) -> Self {
        self.headers = self.headers.overlay(&headers);
        self
    }

    /// Whether requests send the trace context of the current observation
    /// (default: the setting of the [`GatewayClient`]).
    pub fn with_trace_context_propagation(mut self, propagate: bool) -> Self {
        self.propagate_trace_context = propagate;
        self
    }
}

#[cfg(feature = "openai")]
impl async_openai::config::Config for GatewayOpenAIConfig {
    fn headers(&self) -> HeaderMap {
        // Called for every request, in the context of the caller.
        let mut headers = self.inner.headers();
        headers
            .extend(effective_headers(&self.headers, self.propagate_trace_context).to_header_map());
        for (name, value) in &self.extra_headers {
            headers.insert(name, value.clone());
        }
        headers
    }

    fn url(&self, path: &str) -> String {
        self.inner.url(path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        self.inner.query()
    }

    fn api_base(&self) -> &str {
        self.inner.api_base()
    }

    fn api_key(&self) -> &secrecy::SecretString {
        self.inner.api_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(chunks: &[&str]) -> Vec<ServerSentEvent> {
        let mut parser = SseParser::default();
        let mut out = VecDeque::new();
        for chunk in chunks {
            parser.push(chunk.as_bytes(), &mut out);
        }
        parser.finish(&mut out);
        out.into_iter().collect()
    }

    fn event(event: Option<&str>, data: &str) -> ServerSentEvent {
        ServerSentEvent {
            event: event.map(str::to_owned),
            data: data.to_owned(),
            id: None,
        }
    }

    #[test]
    fn sse_parses_named_and_unnamed_events() {
        let parsed = events(&[
            "event: message_start\ndata: {\"a\":1}\n\n",
            ": keep-alive\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n",
        ]);
        assert_eq!(
            parsed,
            vec![
                event(Some("message_start"), "{\"a\":1}"),
                event(None, "{\"b\":2}"),
                event(None, "[DONE]"),
            ]
        );
        assert!(parsed[2].is_done());
    }

    #[test]
    fn sse_handles_split_chunks_and_line_endings() {
        // CRLF split between chunks, CR alone, multi-line data, no space after the colon.
        let parsed = events(&["event: x\r", "\ndata:line1\r\ndata: line2\r\r", "data: é\n"]);
        assert_eq!(
            parsed,
            vec![event(Some("x"), "line1\nline2"), event(None, "é")]
        );
        // A UTF-8 character split between two chunks.
        let mut parser = SseParser::default();
        let mut out = VecDeque::new();
        parser.push(b"data: caf\xc3", &mut out);
        parser.push(b"\xa9\n\n", &mut out);
        assert_eq!(out.pop_front(), Some(event(None, "café")));
    }

    #[test]
    fn sse_keeps_the_last_id_and_ignores_unknown_fields() {
        let parsed = events(&["id: 7\nretry: 10\nfoo\ndata: a\n\ndata: b\n\n"]);
        assert_eq!(parsed[0].id.as_deref(), Some("7"));
        assert_eq!(parsed[1].id.as_deref(), Some("7"));
        assert_eq!(parsed[1].data, "b");
        // An event without data is not dispatched.
        assert!(events(&["event: ping\n\n"]).is_empty());
    }

    #[test]
    fn headers_are_encoded_as_the_gateway_decodes_them() {
        let map = GatewayHeaders::default()
            .with_user_id("jane doe@example.com")
            .with_session_id("s-1")
            .with_environment("ci")
            .with_tags(["a", "b,c", " a ", "", "café"])
            .with_metadata("repo", "docs:main")
            .with_metadata("k:1", "50%, more")
            .with_trace_name("nightly")
            .to_header_map();
        assert_eq!(map["langfuse-user-id"], "jane%20doe@example.com");
        assert_eq!(map["langfuse-session-id"], "s-1");
        assert_eq!(map["langfuse-environment"], "ci");
        assert_eq!(map["langfuse-tags"], "a,b%2Cc,caf%C3%A9");
        assert_eq!(
            map["langfuse-metadata"],
            "k%3A1:50%25%2C%20more,repo:docs:main"
        );
        assert_eq!(map["langfuse-trace-name"], "nightly");
        assert!(!map.contains_key("traceparent"));
    }

    #[test]
    fn invalid_values_are_left_out() {
        let map = GatewayHeaders::default()
            .with_environment("Production")
            .with_traceparent("00-zz-11-01")
            .with_session_id("")
            .to_header_map();
        assert!(map.is_empty());
        let map = GatewayHeaders::default()
            .with_environment("langfuse-x")
            .with_traceparent(format!("00-{}-{}-01", "0".repeat(32), "1".repeat(16)))
            .to_header_map();
        assert!(map.is_empty());
        let valid = format!("00-{}-{}-01", "a".repeat(32), "b".repeat(16));
        let map = GatewayHeaders::default()
            .with_traceparent(valid.clone())
            .with_tracestate("vendor=1")
            .to_header_map();
        assert_eq!(map["traceparent"], valid.as_str());
        assert_eq!(map["tracestate"], "vendor=1");
    }

    #[test]
    fn values_stay_within_the_gateway_limits() {
        let long = "x".repeat(MAX_VALUE_BYTES + 1);
        let map = GatewayHeaders::default()
            .with_session_id(long.clone())
            .with_tags(["keep", long.as_str()])
            .to_header_map();
        assert!(!map.contains_key("langfuse-session-id"));
        assert_eq!(map["langfuse-tags"], "keep");

        // Eight headers of ~1 KiB each exceed 8 KiB: metadata goes first.
        let value = "v".repeat(1000);
        let mut headers = GatewayHeaders::default()
            .with_user_id(value.clone())
            .with_session_id(value.clone())
            .with_trace_name(value.clone())
            .with_tracestate(value.clone())
            .with_traceparent(format!("00-{}-{}-01", "a".repeat(32), "b".repeat(16)))
            .with_tags([value.clone()]);
        for key in ["a", "b", "c", "d", "e", "f", "g", "h"] {
            headers = headers.with_metadata(key, "m".repeat(100));
        }
        let map = headers.to_header_map();
        let total: usize = map
            .iter()
            .map(|(name, value)| name.as_str().len() + value.len())
            .sum();
        assert!(total <= MAX_CONTEXT_BYTES, "{total}");
        assert!(map.contains_key("traceparent"));
        assert!(map.contains_key("langfuse-tags"));
    }

    #[test]
    fn overlay_prefers_the_top_values() {
        let base = GatewayHeaders::default()
            .with_session_id("base")
            .with_user_id("u")
            .with_tags(["a", "b"])
            .with_metadata("k", "1")
            .with_metadata("j", "2");
        let top = GatewayHeaders::default()
            .with_session_id("top")
            .with_tags(["b", "c"])
            .with_metadata("k", "3");
        let merged = base.overlay(&top);
        assert_eq!(merged.session_id.as_deref(), Some("top"));
        assert_eq!(merged.user_id.as_deref(), Some("u"));
        assert_eq!(merged.tags, ["a", "b", "c"]);
        assert_eq!(merged.metadata["k"], "3");
        assert_eq!(merged.metadata["j"], "2");
    }

    #[test]
    fn hosts_and_env() {
        let lookup = |vars: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        let client = GatewayClientBuilder::from_lookup(lookup(&[
            (ENV_GATEWAY_HOST, "https://gateway.example.com/"),
            (ENV_GATEWAY_KEY, " key "),
        ]))
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(client.host(), "https://gateway.example.com");
        assert_eq!(
            client.openai_base_url(),
            "https://gateway.example.com/openai/v1"
        );
        assert_eq!(
            client.anthropic_base_url(),
            "https://gateway.example.com/anthropic"
        );
        assert_eq!(client.key(), "key");
        assert!(!format!("{client:?}").contains("key\""));

        let client = GatewayClientBuilder::from_lookup(lookup(&[
            (ENV_GATEWAY_KEY, "k"),
            ("LANGFUSE_BASE_URL", "https://us.cloud.langfuse.com"),
        ]))
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(client.host(), hosts::US);

        let error = |vars| {
            GatewayClientBuilder::from_lookup(lookup(vars))
                .unwrap_err()
                .to_string()
        };
        assert!(error(&[]).contains(ENV_GATEWAY_KEY));
        assert!(error(&[(ENV_GATEWAY_KEY, "k")]).contains(ENV_GATEWAY_HOST));
        assert!(
            error(&[
                (ENV_GATEWAY_KEY, "k"),
                ("LANGFUSE_BASE_URL", "https://langfuse.example.com")
            ])
            .contains(ENV_GATEWAY_HOST)
        );

        for bad in ["gateway.cloud.langfuse.com", "ftp://x", "https://x/?a=1"] {
            assert!(GatewayClient::new(bad, "k").is_err(), "{bad}");
        }
        assert!(GatewayClient::new(hosts::EU, " ").is_err());
        assert!(
            GatewayClient::builder(hosts::EU, "k")
                .header("bad name", "v")
                .build()
                .is_err()
        );
    }

    #[test]
    fn streaming_bodies_must_be_objects() {
        assert_eq!(
            streaming(&serde_json::json!({"model": "m", "stream": false})).unwrap(),
            serde_json::json!({"model": "m", "stream": true})
        );
        assert!(streaming(&serde_json::json!([1])).is_err());
    }
}
