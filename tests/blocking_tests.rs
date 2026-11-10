//! `blocking::LangfuseClient` (feature `blocking`): requests from plain
//! threads, page iterators, the background queues and parity with the async
//! clients. The mock server runs on a runtime of its own.
#![cfg(feature = "blocking")]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{PUBLIC_KEY, SECRET_KEY, TS, json};
use langfuse_rust_sdk::blocking::LangfuseClient;
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
use langfuse_rust_sdk::pagination::PageParams;
use langfuse_rust_sdk::{ClientConfig, LangfuseError, RequestOptions, RetryPolicy};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer};

/// A mock server on its own runtime, and a blocking client pointed at it.
struct Setup {
    runtime: tokio::runtime::Runtime,
    server: MockServer,
    client: LangfuseClient,
}

impl Setup {
    fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let server = runtime.block_on(MockServer::start());
        let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
            .base_url(server.uri())
            .retry_policy(RetryPolicy {
                max_retries: 0,
                ..RetryPolicy::default()
            })
            .build()
            .unwrap();
        let client = LangfuseClient::new(config).unwrap();
        Self {
            runtime,
            server,
            client,
        }
    }

    fn mount(&self, mock: Mock) {
        self.runtime.block_on(mock.mount(&self.server));
    }

    fn requests(&self, route: &str) -> Vec<wiremock::Request> {
        self.runtime
            .block_on(self.server.received_requests())
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path() == route)
            .collect()
    }
}

fn dataset(name: &str) -> serde_json::Value {
    json!({
        "id": format!("ds-{name}"), "name": name, "description": null, "metadata": null,
        "inputSchema": null, "expectedOutputSchema": null, "projectId": "proj-1",
        "createdAt": TS, "updatedAt": TS
    })
}

#[test]
fn requests_block_and_return_their_result() {
    let setup = Setup::new();
    setup.mount(
        Mock::given(method("GET"))
            .and(path("/api/public/health"))
            .respond_with(json(200, json!({"status": "OK", "version": "3.1.0"}))),
    );
    setup.mount(
        Mock::given(method("GET"))
            .and(path("/api/public/datasets/missing"))
            .respond_with(json(404, json!({"message": "not found"}))),
    );
    let health = setup.client.health.check().unwrap();
    assert_eq!(health.version, "3.1.0");
    assert!(matches!(
        setup.client.datasets.get("missing"),
        Err(LangfuseError::NotFound { .. })
    ));
    // Views and clones share the runtime.
    let view = setup
        .client
        .with_options(RequestOptions::default().with_header("x-view", "1"));
    view.clone().health.check().unwrap();
    let requests = setup.requests("/api/public/health");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].headers.get("x-view").unwrap(), "1");
}

#[test]
fn page_iterators_fetch_one_page_per_step() {
    let setup = Setup::new();
    for page in 1..=2 {
        setup.mount(
            Mock::given(method("GET"))
                .and(path("/api/public/v2/datasets"))
                .and(query_param("page", page.to_string()))
                .respond_with(json(
                    200,
                    json!({"data": [dataset(&format!("d{page}a")), dataset(&format!("d{page}b"))],
                           "meta": {"page": page, "limit": 2, "totalItems": 4, "totalPages": 2}}),
                )),
        );
    }
    let mut pages = setup.client.datasets.list_pages(PageParams::default());
    assert_eq!(pages.next().unwrap().unwrap().data.len(), 2);
    assert_eq!(setup.requests("/api/public/v2/datasets").len(), 1);

    let names: Vec<String> = setup
        .client
        .datasets
        .list_pages(PageParams::default())
        .items()
        .map(|d| d.unwrap().name)
        .collect();
    assert_eq!(names, ["d1a", "d1b", "d2a", "d2b"]);
}

#[test]
fn queued_scores_are_sent_by_the_runtime_worker() {
    let setup = Setup::new();
    setup.mount(
        Mock::given(method("POST"))
            .and(path("/api/public/scores"))
            .respond_with(json(202, json!({"message": "Accepted"}))),
    );
    setup.client.score(CreateScoreRequest {
        trace_id: Some("trace-1".into()),
        ..CreateScoreRequest::new("quality", CreateScoreValue::Numeric(0.8))
    });
    setup.client.flush();
    let sent = setup.requests("/api/public/scores");
    assert_eq!(sent.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body[0]["name"], "quality");
    setup.client.shutdown();
}

#[cfg(feature = "otel")]
#[test]
fn the_tracer_exports_from_synchronous_code() {
    use std::time::Duration;

    use langfuse_rust_sdk::otel::{Observation, ObservationType, TracingOptions};

    let setup = Setup::new();
    setup.mount(
        Mock::given(method("POST"))
            .and(path("/api/public/otel/v1/traces"))
            .respond_with(json(200, json!({}))),
    );
    let tracer = setup
        .client
        .tracer(TracingOptions::default().with_flush_interval(Duration::from_secs(3600)))
        .unwrap();
    let root = tracer.start_observation("sync-root", ObservationType::Span);
    {
        let _current = root.context().attach();
        assert!(Observation::current().is_some());
        tracer
            .start_observation("sync-child", ObservationType::Tool)
            .end();
    }
    root.end();
    tracer.shutdown();

    let exports = setup.requests("/api/public/otel/v1/traces");
    let body = exports
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect::<String>();
    assert!(
        body.contains("sync-root") && body.contains("sync-child"),
        "{body}"
    );
}

#[test]
fn blocking_calls_inside_an_async_runtime_panic() {
    let setup = Setup::new();
    let client = setup.client.clone();
    let outcome = setup.runtime.block_on(async move {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = client.health.check();
        }));
        // Dropping the last handle inside the runtime is fine.
        drop(client);
        result
    });
    assert!(outcome.is_err());
}

/// Methods of the clients in `source`, by client: public ones, and the
/// `fn` lines of `blocking_methods!` when `macro_lines` is set.
fn api_methods(source: &str, macro_lines: bool) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut client = None;
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("impl ")
            && let Some(name) = rest.split([' ', '<', '{']).next()
            && name.ends_with("Client")
        {
            client = Some(name.to_owned());
            continue;
        }
        if line.starts_with('}') {
            client = None;
        }
        let Some(client) = &client else { continue };
        let line = line.trim_start();
        let name = line
            .strip_prefix("pub async fn ")
            .or_else(|| line.strip_prefix("pub fn "))
            .or_else(|| line.strip_prefix("fn ").filter(|_| macro_lines));
        if let Some(name) = name {
            let name: String = name
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            out.entry(client.clone()).or_default().insert(name);
        }
    }
    out
}

#[test]
fn every_async_api_method_has_a_blocking_version() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let blocking = std::fs::read_to_string(root.join("src/blocking/api.rs")).unwrap();
    let blocking: BTreeMap<String, BTreeSet<String>> = api_methods(
        &blocking
            .lines()
            .map(|l| l.strip_prefix("    ").unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n"),
        true,
    );
    let mut checked = 0;
    for entry in std::fs::read_dir(root.join("src/api")).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "mod.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (client, methods) in api_methods(&source, false) {
            let wrapped = blocking.get(&client).cloned().unwrap_or_default();
            for method in methods {
                assert!(
                    wrapped.contains(&method),
                    "{client}::{method} has no blocking version; regenerate src/blocking/api.rs"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 145, "only {checked} methods found");
}
