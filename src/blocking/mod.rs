//! Blocking client (feature `blocking`), for programs without an async
//! runtime.
//!
//! [`LangfuseClient`] has the same sub-clients as [`crate::LangfuseClient`],
//! with blocking methods: `client.prompts.get("name")` returns the prompt
//! instead of a future. List endpoints get `*_pages` iterators.
//!
//! ```rust,no_run
//! use langfuse_rust_sdk::ClientConfig;
//! use langfuse_rust_sdk::blocking::LangfuseClient;
//! use langfuse_rust_sdk::pagination::PageParams;
//!
//! # fn main() -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let client = LangfuseClient::new(ClientConfig::from_env()?)?;
//! let health = client.health.check()?;
//! println!("Langfuse {}", health.version);
//!
//! for dataset in client.datasets.list_pages(PageParams::default()).items() {
//!     println!("{}", dataset?.name);
//! }
//! client.shutdown(); // send queued scores before exiting
//! # Ok(())
//! # }
//! ```
//!
//! # Runtime
//!
//! Like `reqwest::blocking`, the client runs the async SDK on a Tokio runtime
//! of its own, with one worker thread shared by its clones. The worker also
//! runs the background work: the score queue of [`LangfuseClient::score`],
//! media uploads and, with the `otel` feature, the export of
//! [`LangfuseClient::tracer`]. Call [`flush`](LangfuseClient::flush) or
//! [`shutdown`](LangfuseClient::shutdown) (and [`LangfuseTracer::shutdown`]
//! for the tracer) before the program exits: the runtime stops, without
//! waiting, when the last clone is dropped.
//!
//! The blocking methods panic when called from async code (inside a Tokio
//! runtime), like `reqwest::blocking`: use the async client there.
//!
//! Async-only helpers (`run_experiment`, the `in_scope` scopes of
//! observations, ...) run through [`LangfuseClient::block_on`] with the
//! async client of [`LangfuseClient::async_client`]. Observations of a
//! blocking program become current with `observation.context().attach()`,
//! and `#[observe]` (feature `macros`) works on synchronous functions.

pub mod api;

use std::fmt;
use std::future::Future;
use std::sync::Arc;

use futures_util::stream::StreamExt;

pub use api::LangfuseClient;

use crate::config::ClientConfig;
use crate::error::LangfuseError;
use crate::http::RequestOptions;
use crate::models::projects::Project;
use crate::pagination::{Items, Page, Pages};

/// The Tokio runtime of a blocking client, stopped (without waiting) when
/// dropped, so dropping the last client is safe anywhere.
pub(crate) struct Runtime {
    runtime: Option<tokio::runtime::Runtime>,
}

impl Runtime {
    fn new() -> Result<Self, LangfuseError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("langfuse-blocking")
            .enable_all()
            .build()
            .map_err(|e| LangfuseError::Config(format!("Cannot start the Tokio runtime: {e}")))?;
        Ok(Self {
            runtime: Some(runtime),
        })
    }

    fn get(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .as_ref()
            .expect("the runtime lives until dropped")
    }

    /// Run `future` to completion on the runtime.
    ///
    /// # Panics
    ///
    /// When called inside an async runtime.
    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.get().block_on(future)
    }

    /// Enter the runtime, so code that spawns tasks finds it.
    pub(crate) fn enter(&self) -> tokio::runtime::EnterGuard<'_> {
        self.get().enter()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

/// Blocking versions of async methods: `fn name(&self, args) -> T;` becomes
/// a method running `self.inner.name(args)` on the runtime.
macro_rules! blocking_methods {
    ($( $(#[$doc:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) -> $ret:ty; )*) => {
        $(
            $(#[$doc])*
            pub fn $name(&self $(, $arg: $ty)*) -> $ret {
                self.runtime.block_on(self.inner.$name($($arg),*))
            }
        )*
    };
}
pub(crate) use blocking_methods;

/// Iterator over the pages of a list endpoint; the blocking counterpart of
/// [`Pages`]. Each `next` fetches one page.
#[must_use = "iterators do nothing unless consumed"]
pub struct BlockingPages<'a, P> {
    pages: Pages<'a, P>,
    runtime: &'a Runtime,
}

impl<'a, P> BlockingPages<'a, P> {
    pub(crate) fn new(pages: Pages<'a, P>, runtime: &'a Arc<Runtime>) -> Self {
        Self { pages, runtime }
    }
}

impl<'a, P: Page + Send + 'a> BlockingPages<'a, P>
where
    P::Item: Send + 'a,
{
    /// Iterate over the items of every page.
    pub fn items(self) -> BlockingItems<'a, P::Item> {
        BlockingItems {
            items: self.pages.items(),
            runtime: self.runtime,
        }
    }
}

impl<P> Iterator for BlockingPages<'_, P> {
    type Item = Result<P, LangfuseError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.runtime.block_on(self.pages.next())
    }
}

impl<P> fmt::Debug for BlockingPages<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingPages").finish_non_exhaustive()
    }
}

/// Iterator over the items of a list endpoint, created by
/// [`BlockingPages::items`].
#[must_use = "iterators do nothing unless consumed"]
pub struct BlockingItems<'a, T> {
    items: Items<'a, T>,
    runtime: &'a Runtime,
}

impl<T> Iterator for BlockingItems<'_, T> {
    type Item = Result<T, LangfuseError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.runtime.block_on(self.items.next())
    }
}

impl<T> fmt::Debug for BlockingItems<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingItems").finish_non_exhaustive()
    }
}

impl fmt::Debug for LangfuseClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LangfuseClient")
            .field("public_key", &self.inner.public_key())
            .finish_non_exhaustive()
    }
}

impl LangfuseClient {
    /// Create a client and the runtime it runs on.
    pub fn new(config: ClientConfig) -> Result<Self, LangfuseError> {
        let runtime = Arc::new(Runtime::new()?);
        let client = crate::LangfuseClient::new(config)?;
        Ok(Self::assemble(client, runtime))
    }

    /// The async client this one wraps (same connection pool and queues).
    pub fn async_client(&self) -> &crate::LangfuseClient {
        &self.inner
    }

    /// Run any future (e.g. an async-only SDK call on
    /// [`async_client`](Self::async_client)) on the client's runtime and
    /// return its output.
    ///
    /// # Panics
    ///
    /// When called inside an async runtime.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// A view applying `options` to every request; see
    /// [`crate::LangfuseClient::with_options`].
    pub fn with_options(&self, options: RequestOptions) -> LangfuseClient {
        Self::assemble(self.inner.with_options(options), Arc::clone(&self.runtime))
    }

    /// The overrides applied to every request of this client.
    pub fn request_options(&self) -> &RequestOptions {
        self.inner.request_options()
    }

    /// The public API key of this client.
    pub fn public_key(&self) -> &str {
        self.inner.public_key()
    }

    /// Queue a score, sent in the background; see [`crate::LangfuseClient::score`].
    pub fn score(&self, score: crate::models::scores::CreateScoreRequest) {
        let _runtime = self.runtime.enter();
        self.inner.score(score);
    }

    /// Send every queued score and finish every queued media upload.
    pub fn flush(&self) {
        self.runtime.block_on(self.inner.flush());
    }

    /// Flush, then stop the background tasks; see [`crate::LangfuseClient::shutdown`].
    pub fn shutdown(&self) {
        self.runtime.block_on(self.inner.shutdown());
    }

    /// Verify the API keys; see [`crate::LangfuseClient::auth_check`].
    pub fn auth_check(&self) -> Result<Project, LangfuseError> {
        self.runtime.block_on(self.inner.auth_check())
    }

    /// URL of a trace in the Langfuse UI; see [`crate::LangfuseClient::trace_url`].
    pub fn trace_url(&self, trace_id: &str) -> Result<String, LangfuseError> {
        self.runtime.block_on(self.inner.trace_url(trace_id))
    }

    /// A dataset with all its items; see [`crate::LangfuseClient::get_dataset`].
    pub fn get_dataset(
        &self,
        name: &str,
        options: &crate::api::datasets::GetDatasetOptions,
    ) -> Result<crate::models::datasets::DatasetWithItems, LangfuseError> {
        self.runtime.block_on(self.inner.get_dataset(name, options))
    }

    /// Create a dataset item, uploading its media first; see
    /// [`crate::LangfuseClient::create_dataset_item`].
    pub fn create_dataset_item(
        &self,
        request: &crate::models::dataset_items::CreateDatasetItemRequest,
    ) -> Result<crate::models::dataset_items::DatasetItem, LangfuseError> {
        self.runtime
            .block_on(self.inner.create_dataset_item(request))
    }

    /// Score stored observations; see
    /// [`crate::LangfuseClient::run_batched_evaluation`].
    pub fn run_batched_evaluation(
        &self,
        options: crate::batch_evaluation::BatchEvaluationOptions,
    ) -> Result<crate::batch_evaluation::BatchEvaluationResult, LangfuseError> {
        self.runtime
            .block_on(self.inner.run_batched_evaluation(options))
    }

    /// A tracer exporting through this client, running on its runtime
    /// (feature `otel`); see [`crate::LangfuseClient::tracer`].
    #[cfg(feature = "otel")]
    pub fn tracer(
        &self,
        options: crate::otel::TracingOptions,
    ) -> Result<LangfuseTracer, LangfuseError> {
        let _runtime = self.runtime.enter();
        Ok(LangfuseTracer {
            tracer: self.inner.tracer(options)?,
            runtime: Arc::clone(&self.runtime),
        })
    }
}

/// A [`crate::otel::LangfuseTracer`] with blocking [`flush`](Self::flush) and
/// [`shutdown`](Self::shutdown) (feature `otel`). It dereferences to the
/// async tracer for everything else (`start_observation`, `score`, ...).
#[cfg(feature = "otel")]
#[derive(Clone)]
pub struct LangfuseTracer {
    tracer: crate::otel::LangfuseTracer,
    runtime: Arc<Runtime>,
}

#[cfg(feature = "otel")]
impl fmt::Debug for LangfuseTracer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.tracer.fmt(f)
    }
}

#[cfg(feature = "otel")]
impl std::ops::Deref for LangfuseTracer {
    type Target = crate::otel::LangfuseTracer;

    fn deref(&self) -> &Self::Target {
        &self.tracer
    }
}

#[cfg(feature = "otel")]
impl LangfuseTracer {
    /// Export every finished span, then flush the client's queues; see
    /// [`crate::otel::LangfuseTracer::flush`].
    pub fn flush(&self) {
        self.runtime.block_on(self.tracer.flush());
    }

    /// Export every finished span and stop exporting; see
    /// [`crate::otel::LangfuseTracer::shutdown`].
    pub fn shutdown(&self) {
        self.runtime.block_on(self.tracer.shutdown());
    }

    /// The async tracer.
    pub fn as_async(&self) -> &crate::otel::LangfuseTracer {
        &self.tracer
    }
}
