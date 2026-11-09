//! Pagination helpers for list endpoints.

use serde::{Deserialize, Serialize};

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
