# langfuse-sdk

Rust SDK for the [Langfuse](https://langfuse.com) LLM observability API.

## Features

- Async-only API built on `reqwest` + `tokio`
- Typed models with `serde` for all core Langfuse resources
- Automatic retry with exponential backoff, jitter, and `Retry-After` support
- HTTP Basic Auth with SDK identification headers
- Cursor-based and page-based pagination
- Tagged enum deserialization for `ScoreV3` (on `dataType`) and `Prompt` (on `type`)

## Supported Resources

| Client | Endpoints |
|---|---|
| `health` | Health check |
| `scores` | Create, list (cursor), delete |
| `score_configs` | Create, list (page), get |
| `observations` | List (cursor) |
| `datasets` | Create, list (page), get by name |
| `dataset_items` | Create, list (page), get, delete |
| `prompts` | Create, list (page), get, delete |
| `prompt_versions` | Get, update labels |
| `models` | Create, list (page), get, update, delete |
| `skills` | Create version, list (page), get, update, set labels, delete version |
| `projects` | Create, list, update, delete |
| `traces` | Delete single, delete batch |
| `ingestion` | Ingest OTel traces |
| `media` | Get media URL |

## Quick Start

```rust
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
    let datasets = client.datasets.list(
        &langfuse_sdk::pagination::PageParams { page: Some(1), limit: Some(10) },
    ).await?;
    for ds in &datasets.data {
        println!("  {} ({})", ds.name, ds.id);
    }

    // Create a score
    use langfuse_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
    let score = client.scores.create(&CreateScoreRequest {
        name: "accuracy".into(),
        value: CreateScoreValue::Numeric(0.95),
        trace_id: Some("trace-abc".into()),
        id: None, session_id: None, observation_id: None,
        dataset_run_id: None, comment: None, metadata: None,
        environment: None, queue_id: None, data_type: None, config_id: None,
    }).await?;
    println!("Created score: {}", score.id);

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

# Option 2: inline env vars
LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... cargo run --example <name>
```

| Example | Description |
|---|---|
| [`health_check`](examples/health_check.rs) | Connect to Langfuse and verify the server is healthy |
| [`create_score`](examples/create_score.rs) | Create numeric and categorical scores for a trace |
| [`list_scores`](examples/list_scores.rs) | Paginate through all scores using cursor-based pagination |
| [`score_configs`](examples/score_configs.rs) | Create numeric and categorical score configurations |
| [`manage_datasets`](examples/manage_datasets.rs) | Create a dataset, add items, and list all datasets |
| [`prompts`](examples/prompts.rs) | Create text and chat prompts, fetch by name, and list all |
| [`manage_models`](examples/manage_models.rs) | Register custom models with pricing, list, and delete |
| [`skills`](examples/skills.rs) | Create a skill version with files, fetch with content, list all |
| [`observations`](examples/observations.rs) | List recent observations with a formatted table output |
| [`otel_ingestion`](examples/otel_ingestion.rs) | Ingest OpenTelemetry trace spans into Langfuse |
| [`error_handling`](examples/error_handling.rs) | Handle config errors, 404s, auth failures, and graceful fallbacks |

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
| 429 | `RateLimited` (with optional `retry_after` duration) |
| 5xx | `ServerError` |

Retryable errors (429, 408, 409, 5xx) are automatically retried with exponential backoff. See [`error_handling`](examples/error_handling.rs) for patterns.

## Project Structure

```
src/
  lib.rs            — LangfuseClient, crate root, re-exports
  config.rs         — ClientConfig builder
  error.rs          — LangfuseError enum
  http.rs           — HttpClient (reqwest wrapper with auth + retry)
  retry.rs          — RetryPolicy (exponential backoff + jitter)
  pagination.rs     — Paginated<T>, CursorPaginated<T>, query param types
  models/           — Typed request/response structs for all resources
  api/              — Resource clients (one per API group)
examples/           — 10 runnable examples covering all major features
tests/              — 110 unit + integration tests
```

## Requirements

- Rust 1.99+ (edition 2024)
- tokio runtime

## License

MIT
