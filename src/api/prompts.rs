//! Retrieve, create, update and delete prompts, with an optional client-side cache.
//!
//! [`PromptsClient::get_cached`] is the call to use on hot paths: it serves
//! prompts from memory, refreshes expired ones in the background and can fall
//! back to a built-in prompt when Langfuse is unreachable.
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::api::prompts::{GetPromptOptions, PromptFallback};
//! # use langfuse_rust_sdk::LangfuseClient;
//! # use serde_json::json;
//! # async fn example(client: &LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let options = GetPromptOptions::default()
//!     .with_fallback(PromptFallback::Text("Summarize: {{text}}".into()));
//! let prompt = client.prompts.get_cached("summarizer", &options).await?;
//! let text = prompt.compile(&json!({"text": "..."}))?.into_text();
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::LangfuseError;
use crate::http::{HttpClient, RequestOptions, encode_path_segment};
use crate::models::enums::PromptType;
use crate::models::prompts::{BasePrompt, ChatPromptMessage, CreatePromptRequest, Prompt};
use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};
use crate::prompt_cache::{CacheKey, Lookup, PromptCache};

#[derive(Clone)]
pub struct PromptsClient {
    pub(crate) http: Arc<HttpClient>,
    /// Shared with [`PromptVersionClient`](crate::api::prompt_version::PromptVersionClient),
    /// whose updates invalidate it.
    pub(crate) cache: Arc<PromptCache>,
}

/// Which version of a prompt [`PromptsClient::get_cached`] returns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PromptSelector {
    /// The version carrying this label.
    Label(String),
    /// This version number.
    Version(i64),
}

impl PromptSelector {
    /// The version carrying `label`.
    pub fn label(label: impl Into<String>) -> Self {
        Self::Label(label.into())
    }
}

impl Default for PromptSelector {
    /// The version labelled `production`.
    fn default() -> Self {
        Self::Label("production".into())
    }
}

/// Prompt content returned by [`PromptsClient::get_cached`] when the prompt is
/// neither cached nor fetchable. Uses the same `{{variable}}` syntax.
#[derive(Debug, Clone)]
pub enum PromptFallback {
    /// A text prompt.
    Text(String),
    /// A chat prompt.
    Chat(Vec<ChatPromptMessage>),
}

/// Options of [`PromptsClient::get_cached`]; every field has a default.
///
/// Build it from [`GetPromptOptions::default`] with the `with_*` methods, so
/// new options can be added without breaking your code.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct GetPromptOptions {
    /// Label or version to fetch (default: label `production`).
    pub selector: PromptSelector,
    /// How long a fetched prompt is served without refreshing it (default:
    /// [`ConfigBuilder::prompt_cache_ttl`](crate::ConfigBuilder::prompt_cache_ttl),
    /// 60 s). `Duration::ZERO` bypasses the cache.
    pub cache_ttl: Option<Duration>,
    /// Returned (marked [`Prompt::is_fallback`]) when the prompt is not cached
    /// and the fetch fails. Fallbacks are never cached.
    pub fallback: Option<PromptFallback>,
    /// Retries of the fetch instead of the client's
    /// [`RetryPolicy::max_retries`](crate::RetryPolicy::max_retries).
    pub max_retries: Option<u32>,
    /// Timeout of each fetch attempt instead of the client timeout. Timeouts
    /// are retried, so a failing call can take up to
    /// `(max_retries + 1) × fetch_timeout` plus the backoff delays.
    pub fetch_timeout: Option<Duration>,
}

impl GetPromptOptions {
    /// Set [`selector`](Self::selector).
    pub fn with_selector(mut self, selector: PromptSelector) -> Self {
        self.selector = selector;
        self
    }

    /// Fetch the version carrying `label`.
    pub fn with_label(self, label: impl Into<String>) -> Self {
        self.with_selector(PromptSelector::Label(label.into()))
    }

    /// Fetch version `version`.
    pub fn with_version(self, version: i64) -> Self {
        self.with_selector(PromptSelector::Version(version))
    }

    /// Set [`cache_ttl`](Self::cache_ttl).
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = Some(ttl);
        self
    }

    /// Set [`fallback`](Self::fallback).
    pub fn with_fallback(mut self, fallback: PromptFallback) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Set [`max_retries`](Self::max_retries).
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Set [`fetch_timeout`](Self::fetch_timeout).
    pub fn with_fetch_timeout(mut self, timeout: Duration) -> Self {
        self.fetch_timeout = Some(timeout);
        self
    }

    fn request_options(&self) -> RequestOptions<'static> {
        RequestOptions {
            max_retries: self.max_retries,
            timeout: self.fetch_timeout,
            ..RequestOptions::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_updated_at: Option<String>,
    /// JSON-encoded array of filter conditions. When set, it takes precedence over
    /// `name`, `label`, `tag`, `from_updated_at` and `to_updated_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl_page_query!(ListPromptsParams);

/// Metadata returned for each prompt in the list endpoint (no prompt content).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMeta {
    pub name: String,
    pub r#type: PromptType,
    pub versions: Vec<i64>,
    pub labels: Vec<String>,
    pub tags: Vec<String>,
    pub last_updated_at: DateTime<Utc>,
    /// Config of the most recent matching version (`null` when unset).
    #[serde(default)]
    pub last_config: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Resolve prompt dependencies (server default `true`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve: Option<bool>,
}

/// Restrict a prompt deletion to one label or version. With neither set, every
/// version of the prompt is deleted.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePromptParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

impl PromptsClient {
    pub async fn list(
        &self,
        params: &ListPromptsParams,
    ) -> Result<Paginated<PromptMeta>, LangfuseError> {
        self.http
            .get_with_query("/api/public/v2/prompts", params)
            .await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: ListPromptsParams) -> Pages<'_, Paginated<PromptMeta>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    /// `POST /api/public/v2/prompts`: create a prompt or a new version of it.
    /// Invalidates the cached versions of the prompt.
    pub async fn create(&self, req: &CreatePromptRequest) -> Result<Prompt, LangfuseError> {
        let prompt: Prompt = self.http.post("/api/public/v2/prompts", req).await?;
        self.cache.invalidate(prompt.name());
        Ok(prompt)
    }

    pub async fn get(&self, name: &str) -> Result<Prompt, LangfuseError> {
        self.http.get(&prompt_path(name)).await
    }

    pub async fn get_with_params(
        &self,
        name: &str,
        params: &GetPromptParams,
    ) -> Result<Prompt, LangfuseError> {
        self.http.get_with_query(&prompt_path(name), params).await
    }

    /// `DELETE /api/public/v2/prompts/{promptName}`: delete every version (204).
    /// Invalidates the cached versions of the prompt.
    pub async fn delete(&self, name: &str) -> Result<(), LangfuseError> {
        self.http.delete_no_content(&prompt_path(name)).await?;
        self.cache.invalidate(name);
        Ok(())
    }

    /// `DELETE /api/public/v2/prompts/{promptName}?label=..&version=..` (204).
    pub async fn delete_with_params(
        &self,
        name: &str,
        params: &DeletePromptParams,
    ) -> Result<(), LangfuseError> {
        self.http
            .delete_no_content_with_query(&prompt_path(name), params)
            .await?;
        self.cache.invalidate(name);
        Ok(())
    }

    /// Get a prompt through the client-side cache.
    ///
    /// - A fresh cached prompt is returned without a request.
    /// - An expired one is returned immediately while a background task
    ///   (one per prompt and selector) fetches the new version.
    /// - Otherwise the prompt is fetched and cached; if that fails, the
    ///   [`fallback`](GetPromptOptions::fallback) is returned when set, or the
    ///   error otherwise.
    ///
    /// A `404` evicts the prompt from the cache. [`create`](Self::create),
    /// [`delete`](Self::delete) and
    /// [`PromptVersionClient::update`](crate::api::prompt_version::PromptVersionClient::update)
    /// invalidate the cached versions of the prompt they change. The cache is
    /// shared by clones of the client.
    pub async fn get_cached(
        &self,
        name: &str,
        options: &GetPromptOptions,
    ) -> Result<Arc<Prompt>, LangfuseError> {
        let key = CacheKey {
            name: name.to_owned(),
            selector: options.selector.clone(),
        };
        let ttl = options
            .cache_ttl
            .unwrap_or_else(|| self.cache.default_ttl());

        if !ttl.is_zero() {
            match self.cache.lookup(&key) {
                Lookup::Fresh(prompt) => return Ok(prompt),
                Lookup::Stale(prompt) => {
                    self.spawn_refresh(key, ttl, options.request_options());
                    return Ok(prompt);
                }
                Lookup::Miss => {}
            }
        }

        match fetch_and_store(
            &self.http,
            &self.cache,
            &key,
            ttl,
            options.request_options(),
        )
        .await
        {
            Ok(prompt) => Ok(prompt),
            Err(err) => match &options.fallback {
                Some(fallback) => {
                    tracing::warn!(prompt = %key, error = %err, "returning fallback prompt");
                    Ok(Arc::new(fallback_prompt(&key, fallback)))
                }
                None => Err(err),
            },
        }
    }

    /// Remove every prompt from the cache of [`get_cached`](Self::get_cached).
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Refresh `key` in the background unless a refresh is already running.
    fn spawn_refresh(&self, key: CacheKey, ttl: Duration, request: RequestOptions<'static>) {
        // Outside a Tokio runtime there is nothing to spawn on: keep serving the stale prompt.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let Some(guard) = self.cache.start_refresh(&key) else {
            return;
        };
        let http = Arc::clone(&self.http);
        let cache = Arc::clone(&self.cache);
        runtime.spawn(async move {
            let _guard = guard;
            // Another caller may have refreshed it since the lookup.
            if cache.is_fresh(&key) {
                return;
            }
            if let Err(err) = fetch_and_store(&http, &cache, &key, ttl, request).await {
                tracing::warn!(prompt = %key, error = %err, "background prompt refresh failed");
            }
        });
    }
}

/// Fetch the prompt of `key` and cache it, unless the cache was invalidated
/// meanwhile. A `404` evicts the key.
async fn fetch_and_store(
    http: &HttpClient,
    cache: &PromptCache,
    key: &CacheKey,
    ttl: Duration,
    request: RequestOptions<'_>,
) -> Result<Arc<Prompt>, LangfuseError> {
    let generation = cache.generation();
    let params = match &key.selector {
        PromptSelector::Label(label) => GetPromptParams {
            label: Some(label.clone()),
            ..GetPromptParams::default()
        },
        PromptSelector::Version(version) => GetPromptParams {
            version: Some(*version),
            ..GetPromptParams::default()
        },
    };
    match http
        .get_with_query_options::<Prompt, _>(&prompt_path(&key.name), &params, request)
        .await
    {
        Ok(prompt) => {
            let prompt = Arc::new(prompt);
            cache.insert(key.clone(), Arc::clone(&prompt), ttl, generation);
            Ok(prompt)
        }
        Err(err) => {
            if matches!(err, LangfuseError::NotFound { .. }) {
                cache.remove(key);
            }
            Err(err)
        }
    }
}

fn fallback_prompt(key: &CacheKey, fallback: &PromptFallback) -> Prompt {
    let (version, labels) = match &key.selector {
        PromptSelector::Label(label) => (0, vec![label.clone()]),
        PromptSelector::Version(version) => (*version, Vec::new()),
    };
    let base = BasePrompt {
        name: key.name.clone(),
        version,
        config: serde_json::Value::Object(serde_json::Map::new()),
        labels,
        tags: Vec::new(),
        commit_message: None,
        resolution_graph: None,
        is_fallback: true,
    };
    match fallback {
        PromptFallback::Text(text) => Prompt::Text {
            base,
            prompt: text.clone(),
        },
        PromptFallback::Chat(messages) => Prompt::Chat {
            base,
            prompt: messages.clone(),
        },
    }
}

fn prompt_path(name: &str) -> String {
    format!("/api/public/v2/prompts/{}", encode_path_segment(name))
}
