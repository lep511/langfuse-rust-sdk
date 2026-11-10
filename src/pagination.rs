//! Pagination helpers for list endpoints.
//!
//! Every list method that returns [`Paginated`] or [`CursorPaginated`] has a
//! `*_pages` companion (e.g. [`ObservationsClient::list_pages`]) that returns a
//! lazy [`Pages`](crate::pagination::Pages) stream: it requests the next page only when the previous one
//! has been consumed, and ends after the last page or the first error.
//! [`Pages::items`](crate::pagination::Pages::items) flattens the pages into a stream of items.
//!
//! The streams implement [`Stream`](futures_util::stream::Stream); consume them with `StreamExt` /
//! `TryStreamExt` from the `futures` crate. `.take(n)` on [`Items`](crate::pagination::Items) caps the
//! number of items without requesting further pages.
//!
//! ```rust,no_run
//! use futures::TryStreamExt;
//! use langfuse_rust_sdk::LangfuseClient;
//! use langfuse_rust_sdk::api::observations::ListObservationsParams;
//!
//! # async fn example(client: &LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
//! let params = ListObservationsParams {
//!     trace_id: Some("abc123".into()),
//!     // Bound the time window: without it the API searches from 2020-01-01.
//!     from_start_time: Some("2026-11-01T00:00:00Z".into()),
//!     limit: Some(100),
//!     ..Default::default()
//! };
//! let mut observations = client.observations.list_pages(params).items();
//! while let Some(observation) = observations.try_next().await? {
//!     println!("{}", observation.id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! [`ObservationsClient::list_pages`]: crate::api::observations::ObservationsClient::list_pages

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::{self, Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};

use crate::error::LangfuseError;

/// Metadata for page-based pagination responses.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    /// Current page number (1-based).
    pub page: i64,
    /// Maximum items per page.
    pub limit: i64,
    /// Total number of items across all pages.
    pub total_items: i64,
    /// Total number of pages.
    pub total_pages: i64,
}

/// Metadata for cursor-based pagination responses.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorMeta {
    /// Opaque cursor for the next page, or `None` if this is the last page.
    pub cursor: Option<String>,
}

/// A page-based paginated response containing `data` items and [`PageMeta`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paginated<T> {
    /// The items in this page.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub meta: PageMeta,
}

/// A cursor-based paginated response containing `data` items and [`CursorMeta`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPaginated<T> {
    /// The items in this page.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub meta: CursorMeta,
}

/// Query parameters for page-based list endpoints.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageParams {
    /// The page number to fetch (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    /// Maximum number of items per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

/// Query parameters for cursor-based list endpoints.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorParams {
    /// Opaque cursor from a previous response's [`CursorMeta::cursor`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Maximum number of items per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

// ═══════════════════════════════════════════════════════════════════════
//  Streams
// ═══════════════════════════════════════════════════════════════════════

/// A page of a list response that can be flattened into its items.
pub trait Page {
    /// The item type of the page.
    type Item;

    /// Consume the page and return its items.
    fn into_items(self) -> Vec<Self::Item>;
}

impl<T> Page for Paginated<T> {
    type Item = T;

    fn into_items(self) -> Vec<T> {
        self.data
    }
}

impl<T> Page for CursorPaginated<T> {
    type Item = T;

    fn into_items(self) -> Vec<T> {
        self.data
    }
}

/// Query parameters of a page-based list endpoint (have a `page` field).
pub trait PageQuery: Clone {
    /// The requested page (1-based), if set.
    fn page(&self) -> Option<i64>;

    /// Request the given page (1-based).
    fn set_page(&mut self, page: i64);
}

/// Query parameters of a cursor-based list endpoint (have a `cursor` field).
pub trait CursorQuery: Clone {
    /// The requested cursor, if set.
    fn cursor(&self) -> Option<&str>;

    /// Request the page at the given cursor.
    fn set_cursor(&mut self, cursor: Option<String>);
}

/// Implement [`PageQuery`] for structs with a `page: Option<i64>` field.
macro_rules! impl_page_query {
    ($($ty:ty),+ $(,)?) => {$(
        impl $crate::pagination::PageQuery for $ty {
            fn page(&self) -> Option<i64> {
                self.page
            }

            fn set_page(&mut self, page: i64) {
                self.page = Some(page);
            }
        }
    )+};
}

/// Implement [`CursorQuery`] for structs with a `cursor: Option<String>` field.
macro_rules! impl_cursor_query {
    ($($ty:ty),+ $(,)?) => {$(
        impl $crate::pagination::CursorQuery for $ty {
            fn cursor(&self) -> Option<&str> {
                self.cursor.as_deref()
            }

            fn set_cursor(&mut self, cursor: Option<String>) {
                self.cursor = cursor;
            }
        }
    )+};
}

pub(crate) use {impl_cursor_query, impl_page_query};

impl_page_query!(PageParams);
impl_cursor_query!(CursorParams);

type BoxedStream<'a, T> = Pin<Box<dyn Stream<Item = Result<T, LangfuseError>> + Send + 'a>>;

/// Lazy stream of the pages of a list endpoint; see the [module docs](self).
#[must_use = "streams do nothing unless polled"]
pub struct Pages<'a, P> {
    inner: BoxedStream<'a, P>,
}

impl<'a, P: Page + Send + 'a> Pages<'a, P>
where
    P::Item: Send + 'a,
{
    /// Flatten the pages into a stream of their items.
    pub fn items(self) -> Items<'a, P::Item> {
        let inner = self
            .inner
            .map_ok(|page| stream::iter(page.into_items().into_iter().map(Ok)))
            .try_flatten();
        Items {
            inner: Box::pin(inner),
        }
    }
}

impl<P> Stream for Pages<'_, P> {
    type Item = Result<P, LangfuseError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

impl<P> fmt::Debug for Pages<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pages").finish_non_exhaustive()
    }
}

/// Lazy stream of the items of a list endpoint, created by [`Pages::items`].
#[must_use = "streams do nothing unless polled"]
pub struct Items<'a, T> {
    inner: BoxedStream<'a, T>,
}

impl<T> Stream for Items<'_, T> {
    type Item = Result<T, LangfuseError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

impl<T> fmt::Debug for Items<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Items").finish_non_exhaustive()
    }
}

/// Page through a page-based endpoint, starting at `params.page()` (default 1).
///
/// `fetch` receives the parameters of each request with `page` set. The stream
/// ends after page `meta.total_pages` or the first error.
pub fn paginate_pages<'a, T, Q, F, Fut>(params: Q, fetch: F) -> Pages<'a, Paginated<T>>
where
    T: Send + 'a,
    Q: PageQuery + Send + 'a,
    F: FnMut(Q) -> Fut + Send + 'a,
    Fut: Future<Output = Result<Paginated<T>, LangfuseError>> + Send + 'a,
{
    let first = params.page().unwrap_or(1);
    let inner = stream::try_unfold(
        (Some((params, first)), fetch),
        |(next, mut fetch)| async move {
            let Some((mut params, page)) = next else {
                return Ok(None);
            };
            params.set_page(page);
            let response = fetch(params.clone()).await?;
            let next = (page < response.meta.total_pages).then_some((params, page + 1));
            Ok(Some((response, (next, fetch))))
        },
    );
    Pages {
        inner: Box::pin(inner),
    }
}

/// Page through a cursor-based endpoint, starting at `params.cursor()`.
///
/// `fetch` receives the parameters of each request with `cursor` set to the
/// previous response's `meta.cursor`. The stream ends when the cursor is
/// missing or empty, when the server repeats the cursor just requested, or
/// after the first error.
pub fn paginate_cursor<'a, T, Q, F, Fut>(params: Q, fetch: F) -> Pages<'a, CursorPaginated<T>>
where
    T: Send + 'a,
    Q: CursorQuery + Send + 'a,
    F: FnMut(Q) -> Fut + Send + 'a,
    Fut: Future<Output = Result<CursorPaginated<T>, LangfuseError>> + Send + 'a,
{
    let inner = stream::try_unfold((Some(params), fetch), |(next, mut fetch)| async move {
        let Some(mut params) = next else {
            return Ok(None);
        };
        let response = fetch(params.clone()).await?;
        let next = match response.meta.cursor.as_deref() {
            Some(cursor) if !cursor.is_empty() && params.cursor() != Some(cursor) => {
                params.set_cursor(Some(cursor.to_owned()));
                Some(params)
            }
            _ => None,
        };
        Ok(Some((response, (next, fetch))))
    });
    Pages {
        inner: Box::pin(inner),
    }
}
