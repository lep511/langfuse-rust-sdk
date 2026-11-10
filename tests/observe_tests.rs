//! `#[observe]` (feature `macros`): spans, input/output, errors, streams and
//! tracer resolution, checked on the exported OTLP bodies.
#![cfg(feature = "macros")]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::*;
use futures::{Stream, StreamExt};
use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
use langfuse_rust_sdk::observe;
use langfuse_rust_sdk::otel::{
    LangfuseTracer, Observation, ObservationType, TraceContext, TracingOptions, in_trace,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";

async fn setup() -> (LangfuseTracer, MockServer) {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    let options = TracingOptions::default().with_flush_interval(Duration::from_secs(3600));
    (client.tracer(options).unwrap(), server)
}

async fn exported(tracer: &LangfuseTracer, server: &MockServer) -> Vec<Value> {
    tracer.flush().await;
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        for resource_spans in body["resourceSpans"].as_array().unwrap() {
            for scope_spans in resource_spans["scopeSpans"].as_array().unwrap() {
                out.extend(scope_spans["spans"].as_array().unwrap().iter().cloned());
            }
        }
    }
    out
}

fn named<'a>(spans: &'a [Value], name: &str) -> &'a Value {
    let found: Vec<&Value> = spans.iter().filter(|s| s["name"] == name).collect();
    assert_eq!(found.len(), 1, "expected one span named {name}: {spans:#?}");
    found[0]
}

fn attr(span: &Value, key: &str) -> Option<Value> {
    let kv = span["attributes"]
        .as_array()?
        .iter()
        .find(|kv| kv["key"] == key)?;
    ["stringValue", "intValue", "boolValue", "doubleValue"]
        .iter()
        .find_map(|k| kv["value"].get(*k).cloned())
}

fn attr_str(span: &Value, key: &str) -> Option<String> {
    attr(span, key).and_then(|v| v.as_str().map(str::to_owned))
}

/// A JSON attribute (input/output), or a plain string.
fn attr_json(span: &Value, key: &str) -> Option<Value> {
    attr_str(span, key).map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
}

fn kind(span: &Value) -> String {
    attr_str(span, "langfuse.observation.type").unwrap()
}

/// Run `future` inside a root span of `tracer`, which observed functions join.
async fn in_root<F: std::future::Future>(tracer: &LangfuseTracer, future: F) -> F::Output {
    let root = tracer.start_observation("root", ObservationType::Span);
    let output = root.in_scope(future).await;
    root.end();
    output
}

// ── Functions under test ──────────────────────────────────────────────

#[observe(as_type = "retriever")]
async fn search(query: &str, limit: usize) -> Vec<String> {
    (0..limit).map(|i| format!("{query}-{i}")).collect()
}

#[observe(name = "llm-call", as_type = "generation")]
async fn complete(prompt: String) -> Result<String, std::io::Error> {
    if let Some(generation) = Observation::current() {
        generation
            .set_model("gpt-4o")
            .set_usage_details(&json!({"input": 12, "output": 3}));
    }
    Ok(format!("answer to {prompt}"))
}

#[observe]
async fn answer(question: &str) -> Result<String, std::io::Error> {
    let context = search(question, 2).await;
    // `?` and early `return` keep working in the observed body.
    if question.is_empty() {
        return Ok(String::new());
    }
    let answer = complete(context.join(" ")).await?;
    Ok(answer)
}

#[derive(Debug, thiserror::Error)]
#[error("quota exceeded for {0}")]
struct QuotaError(String);

#[observe(as_type = "tool")]
async fn failing_tool(user: &str) -> Result<u32, QuotaError> {
    Err(QuotaError(user.to_owned()))
}

/// Not `Serialize`.
struct Database;

#[observe(skip(api_key))]
fn lookup(db: &Database, api_key: &str, (city, country): (&str, &str)) -> String {
    let _ = (db, api_key);
    format!("{city}, {country}")
}

#[observe(capture_input = false, capture_output = false)]
fn private_step(secret: &str) -> String {
    secret.to_uppercase()
}

struct Service {
    prefix: String,
}

impl Service {
    #[observe(as_type = "chain")]
    async fn run(&self, n: u32) -> String {
        let nested = self.step(n);
        format!("{}{nested}", self.prefix)
    }

    #[observe]
    fn step(&self, n: u32) -> u32 {
        // Observation::current() is the `step` observation in sync code too.
        assert!(Observation::current().is_some());
        n * 2
    }
}

#[observe]
async fn never_finishes() {
    std::future::pending::<()>().await;
}

#[observe]
fn panics() {
    panic!("boom");
}

#[observe(as_type = "generation")]
fn stream_tokens(words: Vec<&'static str>) -> impl Stream<Item = String> + Send {
    futures::stream::iter(words.into_iter().map(str::to_owned))
}

fn first_letters(items: &[Value]) -> String {
    items
        .iter()
        .filter_map(|v| v.as_str()?.chars().next())
        .collect()
}

#[observe(transform_to_string = first_letters)]
async fn stream_with_transform() -> impl Stream<Item = &'static str> {
    futures::stream::iter(["alpha", "beta"])
}

#[observe]
fn stream_with_errors() -> impl Stream<Item = Result<u32, std::io::Error>> {
    futures::stream::iter([Ok(1), Err(std::io::Error::other("bad chunk")), Ok(3)])
}

#[observe]
fn numbers(n: u32) -> impl Iterator<Item = u32> {
    (1..=n).inspect(|_| {
        // Items are produced inside the observation.
        assert!(Observation::current().is_some());
    })
}

#[observe]
fn generic<T: serde::Serialize + Clone>(value: T) -> T {
    value
}

#[observe]
async fn observed_child() -> u32 {
    7
}

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn nested_calls_become_a_tree_with_input_and_output() {
    let (tracer, server) = setup().await;
    let result = in_root(&tracer, answer("orders")).await.unwrap();
    assert_eq!(result, "answer to orders-0 orders-1");

    let spans = exported(&tracer, &server).await;
    let root = named(&spans, "root");
    let answer = named(&spans, "answer");
    let search = named(&spans, "search");
    let llm = named(&spans, "llm-call");
    assert_eq!(answer["parentSpanId"], root["spanId"]);
    assert_eq!(search["parentSpanId"], answer["spanId"]);
    assert_eq!(llm["parentSpanId"], answer["spanId"]);
    assert_eq!(kind(answer), "span");
    assert_eq!(kind(search), "retriever");
    assert_eq!(kind(llm), "generation");

    assert_eq!(
        attr_json(answer, "langfuse.observation.input"),
        Some(json!({"question": "orders"}))
    );
    assert_eq!(
        attr_json(answer, "langfuse.observation.output"),
        Some(json!("answer to orders-0 orders-1"))
    );
    assert_eq!(
        attr_json(search, "langfuse.observation.input"),
        Some(json!({"query": "orders", "limit": 2}))
    );
    assert_eq!(
        attr_json(search, "langfuse.observation.output"),
        Some(json!(["orders-0", "orders-1"]))
    );
    assert_eq!(
        attr_str(llm, "langfuse.observation.model.name").as_deref(),
        Some("gpt-4o")
    );
    assert_eq!(attr(answer, "langfuse.observation.level"), None);
}

#[tokio::test]
async fn err_results_mark_the_observation_as_failed() {
    let (tracer, server) = setup().await;
    let error = in_root(&tracer, failing_tool("u-1")).await.unwrap_err();
    assert_eq!(error.to_string(), "quota exceeded for u-1");

    let spans = exported(&tracer, &server).await;
    let tool = named(&spans, "failing_tool");
    assert_eq!(kind(tool), "tool");
    assert_eq!(
        attr_str(tool, "langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert_eq!(
        attr_str(tool, "langfuse.observation.status_message").as_deref(),
        Some("quota exceeded for u-1")
    );
    assert_eq!(attr(tool, "langfuse.observation.output"), None);
    assert_eq!(tool["status"]["code"], 2, "{tool}");
    let event = &tool["events"][0];
    assert_eq!(event["name"], "exception");
    let event_attr = |key: &str| attr_str(event, key);
    assert_eq!(
        event_attr("exception.message").as_deref(),
        Some("quota exceeded for u-1")
    );
    assert!(
        event_attr("exception.type")
            .unwrap()
            .ends_with("QuotaError"),
        "{event}"
    );
}

#[tokio::test]
async fn non_serializable_skipped_and_pattern_arguments() {
    let (tracer, server) = setup().await;
    in_root(&tracer, async {
        assert_eq!(lookup(&Database, "sk-secret", ("Lima", "PE")), "Lima, PE");
        assert_eq!(private_step("hidden"), "HIDDEN");
        assert_eq!(generic(json!({"k": 1})), json!({"k": 1}));
    })
    .await;

    let spans = exported(&tracer, &server).await;
    let lookup = named(&spans, "lookup");
    let input = attr_json(lookup, "langfuse.observation.input").unwrap();
    assert!(input.get("api_key").is_none(), "{input}");
    assert_eq!(input["(city, country)"], json!(["Lima", "PE"]));
    let db = input["db"].as_str().unwrap();
    assert!(db.starts_with('<') && db.ends_with("Database>"), "{db}");
    assert_eq!(
        attr_json(lookup, "langfuse.observation.output"),
        Some(json!("Lima, PE"))
    );

    let private = named(&spans, "private_step");
    assert_eq!(attr(private, "langfuse.observation.input"), None);
    assert_eq!(attr(private, "langfuse.observation.output"), None);

    let generic = named(&spans, "generic");
    assert_eq!(
        attr_json(generic, "langfuse.observation.output"),
        Some(json!({"k": 1}))
    );
}

#[tokio::test]
async fn methods_leave_self_out_and_sync_calls_nest() {
    let (tracer, server) = setup().await;
    let service = Service {
        prefix: "n=".into(),
    };
    assert_eq!(in_root(&tracer, service.run(21)).await, "n=42");

    let spans = exported(&tracer, &server).await;
    let run = named(&spans, "run");
    let step = named(&spans, "step");
    assert_eq!(kind(run), "chain");
    assert_eq!(step["parentSpanId"], run["spanId"]);
    assert_eq!(
        attr_json(run, "langfuse.observation.input"),
        Some(json!({"n": 21}))
    );
    assert_eq!(
        attr_json(step, "langfuse.observation.output"),
        Some(json!(42))
    );
}

#[tokio::test]
async fn cancelled_futures_and_panics_are_errors() {
    let (tracer, server) = setup().await;
    in_root(&tracer, async {
        let timed_out = tokio::time::timeout(Duration::from_millis(10), never_finishes()).await;
        assert!(timed_out.is_err());
        let panicked = std::panic::catch_unwind(panics);
        assert!(panicked.is_err());
    })
    .await;

    let spans = exported(&tracer, &server).await;
    for (name, message) in [("never_finishes", "cancelled"), ("panics", "panicked")] {
        let span = named(&spans, name);
        assert_eq!(
            attr_str(span, "langfuse.observation.level").as_deref(),
            Some("ERROR"),
            "{name}"
        );
        assert_eq!(
            attr_str(span, "langfuse.observation.status_message").as_deref(),
            Some(message)
        );
    }
}

#[tokio::test]
async fn streams_keep_the_observation_open_until_they_end() {
    let (tracer, server) = setup().await;
    let finished = AtomicBool::new(false);
    in_root(&tracer, async {
        let stream = stream_tokens(vec!["Hel", "lo"]);
        let tokens: Vec<String> = stream.collect().await;
        assert_eq!(tokens, ["Hel", "lo"]);

        let letters: Vec<&str> = stream_with_transform().await.collect().await;
        assert_eq!(letters, ["alpha", "beta"]);

        let results: Vec<_> = stream_with_errors().collect().await;
        assert_eq!(results.len(), 3);

        // Dropped early: the items produced so far are the output.
        let mut early = Box::pin(stream_tokens(vec!["a", "b", "c"]));
        assert_eq!(early.next().await.as_deref(), Some("a"));
        drop(early);
        finished.store(true, Ordering::SeqCst);
    })
    .await;
    assert!(finished.load(Ordering::SeqCst));

    let spans = exported(&tracer, &server).await;
    let tokens: Vec<&Value> = spans
        .iter()
        .filter(|s| s["name"] == "stream_tokens")
        .collect();
    assert_eq!(tokens.len(), 2);
    let outputs: Vec<Value> = tokens
        .iter()
        .map(|s| attr_json(s, "langfuse.observation.output").unwrap())
        .collect();
    assert!(outputs.contains(&json!("Hello")), "{outputs:?}");
    assert!(outputs.contains(&json!("a")), "{outputs:?}");
    assert_eq!(kind(tokens[0]), "generation");

    let transformed = named(&spans, "stream_with_transform");
    assert_eq!(
        attr_json(transformed, "langfuse.observation.output"),
        Some(json!("ab"))
    );
    let with_errors = named(&spans, "stream_with_errors");
    assert_eq!(
        attr_str(with_errors, "langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert_eq!(
        attr_str(with_errors, "langfuse.observation.status_message").as_deref(),
        Some("bad chunk")
    );
    assert_eq!(
        attr_json(with_errors, "langfuse.observation.output"),
        Some(json!([1, 3]))
    );
}

#[tokio::test]
async fn iterators_record_their_items() {
    let (tracer, server) = setup().await;
    in_root(&tracer, async {
        assert_eq!(numbers(3).sum::<u32>(), 6);
        assert_eq!(numbers(5).take(2).collect::<Vec<_>>(), [1, 2]);
    })
    .await;
    let spans = exported(&tracer, &server).await;
    let mut outputs: Vec<Value> = spans
        .iter()
        .filter(|s| s["name"] == "numbers")
        .map(|s| attr_json(s, "langfuse.observation.output").unwrap())
        .collect();
    outputs.sort_by_key(|v| v.as_array().unwrap().len());
    assert_eq!(outputs, [json!([1, 2]), json!([1, 2, 3])]);
}

#[tokio::test]
async fn in_trace_attaches_a_call_to_an_existing_trace() {
    let (tracer, server) = setup().await;
    let trace_id = create_trace_id(Some("observe-in-trace"));
    let parent_id = create_observation_id(None);
    let trace = TraceContext {
        trace_id: trace_id.clone(),
        parent_observation_id: Some(parent_id.clone()),
    };
    let root = tracer.start_observation("holder", ObservationType::Span);
    // The trace context wins over the current observation for the first span.
    root.in_scope(in_trace(trace, async {
        let continued = tracer.start_observation("continued", ObservationType::Span);
        continued.in_scope(observed_child()).await;
        continued.end();
    }))
    .await;
    root.end();

    let spans = exported(&tracer, &server).await;
    let continued = named(&spans, "continued");
    assert_eq!(continued["traceId"], json!(trace_id));
    assert_eq!(continued["parentSpanId"], json!(parent_id));
    assert_eq!(
        attr(continued, "langfuse.internal.as_root"),
        Some(json!(true))
    );
    let child = named(&spans, "observed_child");
    assert_eq!(child["traceId"], json!(trace_id));
    assert_eq!(child["parentSpanId"], continued["spanId"]);
    assert_eq!(attr(child, "langfuse.internal.as_root"), None);
}

/// The only test touching the global tracer (shared by the whole binary).
#[tokio::test]
async fn the_global_tracer_is_used_outside_observations() {
    let (tracer, server) = setup().await;
    // Without a tracer the function just runs.
    assert!(LangfuseTracer::global().is_none());
    assert_eq!(observed_child().await, 7);

    assert!(tracer.install_global().is_none());
    assert_eq!(observed_child().await, 7);
    let trace_id = create_trace_id(None);
    in_trace(TraceContext::new(trace_id.clone()), observed_child()).await;
    assert!(LangfuseTracer::uninstall_global().is_some());
    assert_eq!(observed_child().await, 7);

    let spans = exported(&tracer, &server).await;
    let children: Vec<&Value> = spans
        .iter()
        .filter(|s| s["name"] == "observed_child")
        .collect();
    assert_eq!(children.len(), 2, "{spans:#?}");
    assert!(children.iter().any(|s| s["traceId"] == json!(trace_id)
        && attr(s, "langfuse.internal.as_root") == Some(json!(true))));
}
