# langfuse-sdk

Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.

## Features

- Covers all 120 operations of the Langfuse public API (`openapi.yml`); the 21 unstable ones are behind the `unstable` feature
- Async-only API built on `reqwest` + `tokio`
- Typed models with `serde`, including tagged unions (`ScoreV3` on `dataType`, `Prompt` on `type`, `Evaluator` on `type`, ...)
- Response enums tolerate new server values (`Unknown` variant)
- Automatic retry with exponential backoff, jitter, and `Retry-After` support
- HTTP Basic Auth with SDK identification headers
- Cursor-based and page-based pagination

## Supported Resources

| Client | Endpoints |
|---|---|
| `health` | Health check |
| `scores` | Create (batch), list (cursor), delete |
| `score_configs` | Create, list (page), get, update |
| `observations` | List (cursor) |
| `datasets` | Create, list (page), get by name |
| `dataset_items` | Create, list (page), get, delete |
| `prompts` | Create, list (page), get, delete (all versions or by label/version) |
| `prompt_versions` | Update labels |
| `models` | Create, list (page), get, upsert, delete |
| `skills` | Create version, list (page), get, update tags, set labels, delete version, get file contents |
| `projects` | Create, list, update, delete, list/create/delete API keys |
| `traces` | Delete single, delete batch |
| `ingestion` | Ingest OTel traces |
| `media` | Get, patch upload status, get upload URL |
| `comments` | Create, list (page), get |
| `feedback` | Submit |
| `annotation_queues` | Queues (create, list, get), items (create, list, get, update, delete), assignments (create, delete) |
| `blob_storage_integrations` | List, upsert, get status, delete |
| `evaluation_rules` | Create, list (cursor), get, update, delete |
| `evaluators` | Create, list (cursor), get, update, delete, list versions |
| `experiments` | List experiments, list experiment items (cursor) |
| `llm_connections` | List, upsert, delete (project and organization level) |
| `metrics` | Query (typed `MetricsQuery`) |
| `organizations` | Memberships (organization and project), projects, API keys |
| `scim` | Service provider config, resource types, schemas, users (list, create, get, delete) |
| `unstable_dashboard_widgets` | List, create, get, update, delete (feature `unstable`) |
| `unstable_dashboards` | Dashboards and placements (feature `unstable`) |
| `unstable_topics` | Facets, topics, rules, summaries, assignments (feature `unstable`) |

Organization-level endpoints (`organizations`, `scim`, `blob_storage_integrations`, project API keys, organization LLM connections) need an organization-scoped API key.

The unstable APIs may change without notice upstream. Enable them with:

```toml
[dependencies]
langfuse-sdk = { version = "0.2", features = ["unstable"] }
```

## Quick Start

```rust
use langfuse_sdk::models::scores::{CreateScoreRequest, CreateScoreValue, CreateScoresResponse};
use langfuse_sdk::pagination::PageParams;
use langfuse_sdk::{ClientConfig, LangfuseClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::builder("pk-lf-...", "sk-lf-...")
        .base_url("https://cloud.langfuse.com")
        .build()?;

    let client = LangfuseClient::new(config)?;

    // Health check
    let health = client.health.check().await?;
    println!("Langfuse {} — {}", health.version, health.status);

    // List datasets
    let datasets = client
        .datasets
        .list(&PageParams { page: Some(1), limit: Some(10) })
        .await?;
    for ds in &datasets.data {
        println!("  {} ({})", ds.name, ds.id);
    }

    // Create scores (the endpoint takes a batch)
    let score = CreateScoreRequest {
        trace_id: Some("trace-abc".into()),
        ..CreateScoreRequest::new("accuracy", CreateScoreValue::Numeric(0.95))
    };
    match client.scores.create(&[score]).await? {
        CreateScoresResponse::Accepted(resp) => println!("Scores {}", resp.message),
        CreateScoresResponse::BatchResults(r) => {
            println!("{} accepted, {} rejected", r.accepted, r.rejected)
        }
    }

    Ok(())
}
```

## Examples

All examples are in the `examples/` directory. They auto-load a `.env` file via `dotenvy`, so you can either export environment variables or create a `.env` file:

```sh
# Option 1: .env file (recommended)
cat > .env <<EOF
LANGFUSE_PUBLIC_KEY=pk-lf-...
LANGFUSE_SECRET_KEY=sk-lf-...
LANGFUSE_BASE_URL=https://cloud.langfuse.com
EOF

cargo run --example <name>
cargo run --example dashboards --features unstable   # unstable APIs

# Option 2: inline env vars
LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example <name>
```

| Example | Description |
|---|---|
| [`health_check`](examples/health_check.rs) | Connect to Langfuse and verify the server is healthy |
| [`create_score`](examples/create_score.rs) | Create numeric and categorical scores for a trace in one batch |
| [`list_scores`](examples/list_scores.rs) | Paginate through all scores using cursor-based pagination |
| [`score_configs`](examples/score_configs.rs) | Create numeric and categorical score configurations |
| [`manage_datasets`](examples/manage_datasets.rs) | Create a dataset, add items, and list all datasets |
| [`prompts`](examples/prompts.rs) | Create text and chat prompts (with a placeholder), fetch by label, and list all |
| [`manage_models`](examples/manage_models.rs) | Register custom models with pricing, list, and delete |
| [`skills`](examples/skills.rs) | Create a skill version with files, fetch with content, list all |
| [`observations`](examples/observations.rs) | List recent observations with a formatted table output |
| [`otel_ingestion`](examples/otel_ingestion.rs) | Ingest OpenTelemetry trace spans into Langfuse |
| [`error_handling`](examples/error_handling.rs) | Handle config errors, 404s, auth failures, and graceful fallbacks |
| [`metrics_query`](examples/metrics_query.rs) | Query cost and tokens per model and a daily trend with the Metrics API (read-only) |
| [`experiments`](examples/experiments.rs) | List recent experiments with scores and inspect their items (read-only) |
| [`comments`](examples/comments.rs) | Comment on a trace, then list and fetch its comments (writes a comment) |
| [`annotation_queues`](examples/annotation_queues.rs) | Queue a trace for human review, complete it, and remove the item (writes data) |
| [`evaluators`](examples/evaluators.rs) | Create an LLM-as-a-judge evaluator and a disabled evaluation rule, then delete both |
| [`prompt_labels`](examples/prompt_labels.rs) | Create prompt versions in a folder, promote one to `production`, filter, and delete |
| [`llm_connections`](examples/llm_connections.rs) | List LLM connections; optionally upsert and delete a temporary one |
| [`organization_admin`](examples/organization_admin.rs) | Projects, members, API keys and blob storage exports (organization key, read-only) |
| [`scim_users`](examples/scim_users.rs) | SCIM provider config and paginated user list (organization key, read-only) |
| [`dashboards`](examples/dashboards.rs) | Build a dashboard with three widgets, pause so you can open it in the UI, then delete it (`--features unstable`) |

## Configuration

```rust
use langfuse_sdk::{ClientConfig, RetryPolicy};
use std::time::Duration;

let config = ClientConfig::builder("pk-lf-...", "sk-lf-...")
    .base_url("https://self-hosted.example.com")  // default: https://cloud.langfuse.com
    .timeout(Duration::from_secs(60))              // default: 30s
    .retry_policy(RetryPolicy {
        initial_backoff: Duration::from_millis(500),
        max_backoff: Duration::from_secs(30),
        max_retries: 3,                            // default: 2
        jitter_fraction: 0.2,
    })
    .header("x-custom-header", "value")
    .build()
    .unwrap();
```

## Error Handling

All API methods return `Result<T, LangfuseError>`. The error type maps HTTP status codes to specific variants:

| Status | Variant |
|---|---|
| 400 | `BadRequest` |
| 401 | `Unauthorized` |
| 403 | `AccessDenied` |
| 404 | `NotFound` |
| 405 | `MethodNotAllowed` |
| 409 | `Conflict` |
| 412 | `PreconditionFailed` |
| 429 | `RateLimited` (with optional `retry_after` duration) |
| 503 | `ServiceUnavailable` |
| other 5xx | `ServerError` |

`LangfuseError` is `#[non_exhaustive]`, so `match` statements need a wildcard arm. `err.body()` returns the parsed JSON error body. The evaluators, evaluation-rules and unstable endpoints return a structured body that `PublicApiError::from_error(&err)` (or `UnstablePublicApiError::from_error`) parses.

Requests that fail with 408, 429 or 5xx are retried with exponential backoff, honoring `Retry-After`. 409 is not retried, because it signals a real conflict. See [`error_handling`](examples/error_handling.rs) for patterns.

## Project Structure

```
src/
  lib.rs            — LangfuseClient, crate root, re-exports
  config.rs         — ClientConfig builder
  error.rs          — LangfuseError enum
  http.rs           — HttpClient (reqwest wrapper with auth, retry, path encoding)
  retry.rs          — RetryPolicy (exponential backoff + jitter)
  pagination.rs     — Paginated<T>, CursorPaginated<T>, query param types
  models/           — Typed request/response structs for all resources
  api/              — Resource clients (one per API group)
examples/           — 21 runnable examples covering all major features
tests/              — wiremock HTTP tests (api_*), serde tests, config tests
                      (313 tests in total with --all-features, 287 without)
```

## Requirements

- Rust 1.99+ (edition 2024)
- tokio runtime

## License

MIT
