# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust SDK for the Langfuse LLM observability API. Async-only, built on `reqwest` + `tokio`. Targets Rust 1.99+ (edition 2024).

## Build & Test Commands

```bash
cargo build                          # Build the library
cargo test                           # Run all tests (unit + integration, ~75 tests)
cargo test --test config_tests       # Run only config/client integration tests
cargo test --test serde_tests        # Run only serialization tests
cargo test --lib                     # Run only in-crate unit tests (retry, yaml_tool)
cargo test yaml_tool                 # Run a specific test by name substring
cargo run --example health_check     # Run an example (needs .env with credentials)
cargo clippy                         # Lint
```

Tests are pure unit/serde tests with no network calls. Examples in `examples/` hit the real Langfuse API and require env vars (`LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL`) via `.env` or environment.

## Architecture

**Entry point:** `LangfuseClient` in `src/lib.rs` — constructed from a `ClientConfig`, it holds 14 resource sub-clients (health, scores, datasets, prompts, etc.) that each wrap a shared `Arc<HttpClient>`.

**Core layers:**

- `src/config.rs` — Builder pattern (`ClientConfig::builder(pk, sk).base_url(...).build()`). Produces HTTP Basic Auth header, SDK identification headers (`x-langfuse-sdk-name`, `x-langfuse-sdk-version`, `x-langfuse-public-key`).
- `src/http.rs` — `HttpClient` wraps `reqwest::Client` with auth, default headers, and retry loop. Provides typed methods: `get`, `get_with_query`, `post`, `put`, `patch`, `delete_no_content`, `delete_with_body`. All methods include the retry loop inline.
- `src/retry.rs` — `RetryPolicy` with exponential backoff + jitter. Retryable statuses: 408, 409, 429, 5xx. Honors `Retry-After` header.
- `src/error.rs` — `LangfuseError` enum maps HTTP status codes to typed variants (BadRequest, Unauthorized, NotFound, RateLimited, etc.). `map_response_error` extracts `message` from JSON response body.
- `src/pagination.rs` — Two pagination styles: `Paginated<T>` (page-based with `PageMeta`) and `CursorPaginated<T>` (cursor-based with `CursorMeta`).

**API clients** (`src/api/`): One file per resource group. Each struct holds `Arc<HttpClient>` and provides async methods that call HTTP methods with the correct path and types. API paths use `/api/public/` prefix.

**Models** (`src/models/`): Request/response structs with serde derives. Key patterns:
- All response structs use `#[serde(rename_all = "camelCase")]`
- Optional request fields use `#[serde(skip_serializing_if = "Option::is_none")]`
- Enums in `enums.rs` use `SCREAMING_SNAKE_CASE` (except `PromptType` which uses `lowercase`)
- Tagged unions: `ScoreV3` dispatches on `dataType`, `Prompt` dispatches on `type` — both use `#[serde(tag = "...")]` with `#[serde(flatten)]` for shared base fields

**Tools** (`src/tools/`): `YamlTool` — standalone YAML utility (read/write/merge/validate/frontmatter extraction) using `serde_yml`. Not part of the API client.

## Conventions

- All API methods return `Result<T, LangfuseError>`
- Response structs are `Deserialize`-only; request structs are `Serialize`-only (or both for value types like `ChatMessage`)
- `HttpClient` has two response patterns: `request<T>` returns deserialized JSON, `request_no_content` returns `()` for DELETE/204 endpoints
- Adding a new resource requires: model structs in `src/models/`, API client in `src/api/`, wire both into their respective `mod.rs`, add field to `LangfuseClient` in `src/lib.rs`
