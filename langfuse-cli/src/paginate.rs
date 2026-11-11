//! `--all`: fetch every page of a list, as the official CLI's `callAllPages`
//! (same caps, stderr notices and merged `{data, meta}` result), optionally
//! streaming the items as NDJSON while the pages arrive.

use langfuse_rust_sdk::http::HttpClient;
use serde_json::{Map, Value};

use crate::config::Settings;
use crate::error::Result;
use crate::execute::{ApiResult, execute};
use crate::operation::{Pagination, PreparedRequest};
use crate::output::{Io, write_ndjson_items};

/// Most requests one `--all` run sends, whatever `--max-items` allows: a small
/// `--limit` must not turn an item cap into a request storm.
pub const MAX_PAGES: u64 = 100;

fn notice(io: &mut Io<'_>, text: &str) {
    let _ = writeln!(io.stderr, "{text}");
}

fn as_number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// Fetch every page of `request`, up to `max_items` items and [`MAX_PAGES`]
/// requests. With `stream`, items are written as NDJSON as they arrive and
/// the returned result has no body.
pub async fn all_pages(
    http: &HttpClient,
    settings: &Settings,
    mut request: PreparedRequest,
    pagination: Pagination,
    max_items: u64,
    stream: bool,
    io: &mut Io<'_>,
) -> Result<ApiResult> {
    let max_items = usize::try_from(max_items).unwrap_or(usize::MAX);
    let mut items: Vec<Value> = Vec::new();
    let mut fetched = 0usize;
    let mut pages = 0u64;
    let mut truncated = false;
    let mut total_items: Option<f64> = None;
    if pagination == Pagination::Page && request.query_value("page").is_none() {
        request.set_query("page", "1");
    }
    let mut last: Option<ApiResult>;

    loop {
        let result = execute(http, settings, &request).await?;
        if !result.ok {
            if pages > 0 {
                notice(
                    io,
                    &format!(
                        "--all aborted after {pages} page(s) and {fetched} item(s): HTTP {}",
                        result.status
                    ),
                );
            }
            return Ok(result);
        }
        pages += 1;
        let record = result.body.as_ref().and_then(Value::as_object).cloned();
        let data = record
            .as_ref()
            .and_then(|record| record.get("data"))
            .and_then(Value::as_array)
            .cloned();
        // Not a list response: behave like a single call.
        let Some(data) = data else {
            if pages == 1 {
                return Ok(result);
            }
            last = Some(result);
            break;
        };
        let meta = record
            .as_ref()
            .and_then(|record| record.get("meta").or_else(|| record.get("pagination")))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        let page_len = data.len();
        let take = page_len.min(max_items.saturating_sub(fetched));
        if stream {
            write_ndjson_items(&data[..take], io)?;
        } else {
            items.extend(data.into_iter().take(take));
        }

        let mut next_cursor = None;
        let has_more = match pagination {
            Pagination::Cursor => {
                next_cursor = meta
                    .get("cursor")
                    .and_then(Value::as_str)
                    .filter(|cursor| !cursor.is_empty())
                    .map(str::to_owned);
                next_cursor.is_some()
            }
            Pagination::Page => {
                let page = as_number(meta.get("page")).or_else(|| {
                    request
                        .query_value("page")
                        .and_then(|page| page.parse().ok())
                });
                let total_pages = as_number(meta.get("totalPages"));
                if let Some(total) = as_number(meta.get("totalItems")) {
                    total_items = Some(total);
                }
                match (page, total_pages) {
                    (Some(page), Some(total_pages)) if page < total_pages => {
                        request.set_query("page", format!("{}", page as i64 + 1));
                        true
                    }
                    _ => false,
                }
            }
        };
        last = Some(result);

        if fetched + page_len >= max_items {
            truncated = has_more || fetched + page_len > max_items;
            fetched += take;
            if truncated {
                notice(
                    io,
                    &format!(
                        "--all stopped at {fetched} item(s) (--max-items {max_items}); more data is available"
                    ),
                );
            }
            break;
        }
        fetched += take;
        if !has_more {
            break;
        }
        if pages >= MAX_PAGES {
            truncated = true;
            notice(
                io,
                &format!(
                    "--all stopped after {MAX_PAGES} requests with {fetched} item(s); raise --limit for larger pages or --max-items to continue"
                ),
            );
            break;
        }
        if let Some(cursor) = next_cursor {
            request.set_query("cursor", cursor);
        }
    }

    let last = last.expect("at least one page was fetched");
    if stream {
        return Ok(ApiResult { body: None, ..last });
    }
    let mut meta = Map::new();
    meta.insert("pages".into(), Value::from(pages));
    meta.insert("fetchedItems".into(), Value::from(fetched));
    if let Some(total) = total_items {
        let total = if total.fract() == 0.0 {
            Value::from(total as i64)
        } else {
            Value::from(total)
        };
        meta.insert("totalItems".into(), total);
    }
    meta.insert("truncated".into(), Value::Bool(truncated));
    let mut body = Map::new();
    body.insert("data".into(), Value::Array(items));
    body.insert("meta".into(), Value::Object(meta));
    Ok(ApiResult {
        status: last.status,
        headers: last.headers,
        ok: true,
        body: Some(Value::Object(body)),
    })
}
