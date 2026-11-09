//! `LangfuseClient::score`: background batching of scores.
mod common;

use std::time::Duration;

use common::*;
use langfuse_rust_sdk::config::ScoreQueueOptions;
use langfuse_rust_sdk::models::scores::{CreateScoreRequest, CreateScoreValue};
use langfuse_rust_sdk::{ClientConfig, LangfuseClient, LangfuseError, RetryPolicy};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

const SCORES_PATH: &str = "/api/public/scores";

fn queue(flush_at: usize, flush_interval: Duration, max_batch_bytes: usize) -> ScoreQueueOptions {
    ScoreQueueOptions::default()
        .with_flush_at(flush_at)
        .with_flush_interval(flush_interval)
        .with_max_batch_bytes(max_batch_bytes)
}

/// Only sends on `flush` unless `flush_at` is reached.
fn manual() -> ScoreQueueOptions {
    queue(100, Duration::from_secs(3600), 1_000_000)
}

async fn setup(options: ScoreQueueOptions, retries: u32) -> (LangfuseClient, MockServer) {
    let server = MockServer::start().await;
    let config = ClientConfig::builder(PUBLIC_KEY, SECRET_KEY)
        .base_url(server.uri())
        .retry_policy(RetryPolicy {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            max_retries: retries,
            jitter_fraction: 0.0,
        })
        .score_queue(options)
        .build()
        .unwrap();
    (LangfuseClient::new(config).unwrap(), server)
}

async fn accept_scores(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(SCORES_PATH))
        .respond_with(json(202, json!({"message": "Accepted"})))
        .mount(server)
        .await;
}

/// Bodies (score arrays) of every `POST /api/public/scores`.
async fn batches(server: &MockServer) -> Vec<Vec<Value>> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == SCORES_PATH)
        .map(|r| serde_json::from_slice::<Vec<Value>>(&r.body).unwrap())
        .collect()
}

async fn wait_for_batches(server: &MockServer, count: usize) {
    for _ in 0..150 {
        if batches(server).await.len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("expected {count} score batches");
}

fn numeric(name: &str, value: f64) -> CreateScoreRequest {
    CreateScoreRequest {
        trace_id: Some("trace-1".into()),
        ..CreateScoreRequest::new(name, CreateScoreValue::Numeric(value))
    }
}

#[tokio::test]
async fn queued_scores_are_sent_in_one_batch_on_flush() {
    let (client, server) = setup(manual(), 0).await;
    accept_scores(&server).await;

    client.score(numeric("accuracy", 0.9));
    client.score(CreateScoreRequest {
        id: Some("my-score-id".into()),
        comment: Some("looks right".into()),
        ..CreateScoreRequest::new("verdict", CreateScoreValue::String("pass".into()))
    });
    assert!(
        batches(&server).await.is_empty(),
        "nothing is sent before flush"
    );
    client.flush().await;

    let sent = batches(&server).await;
    assert_eq!(sent.len(), 1);
    let scores = &sent[0];
    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0]["name"], "accuracy");
    assert_eq!(scores[0]["value"], 0.9);
    assert_eq!(scores[0]["traceId"], "trace-1");
    let generated_id = scores[0]["id"].as_str().unwrap();
    assert_eq!(generated_id.len(), 16, "an ID is generated");
    assert!(
        scores[0]["timestamp"].as_str().is_some(),
        "a timestamp is set"
    );
    assert_eq!(scores[1]["id"], "my-score-id");
    assert_eq!(scores[1]["value"], "pass");
    assert_eq!(scores[1]["comment"], "looks right");
}

#[tokio::test]
async fn flush_at_sends_without_flush() {
    let (client, server) = setup(queue(2, Duration::from_secs(3600), 1_000_000), 0).await;
    accept_scores(&server).await;

    for i in 0..5 {
        client.score(numeric(&format!("s{i}"), 1.0));
    }
    wait_for_batches(&server, 2).await;
    let sent = batches(&server).await;
    assert_eq!(sent[0].len(), 2);
    assert_eq!(sent[1].len(), 2);
    client.flush().await;
    assert_eq!(
        batches(&server).await.len(),
        3,
        "the last score goes out on flush"
    );
}

#[tokio::test]
async fn flush_interval_sends_periodically() {
    let (client, server) = setup(queue(100, Duration::from_millis(50), 1_000_000), 0).await;
    accept_scores(&server).await;
    client.score(numeric("periodic", 1.0));
    wait_for_batches(&server, 1).await;
}

#[tokio::test]
async fn batches_stay_under_the_size_limit_and_oversized_scores_are_dropped() {
    let (client, server) = setup(queue(100, Duration::from_secs(3600), 600), 0).await;
    accept_scores(&server).await;

    let with_comment = |name: &str, chars: usize| CreateScoreRequest {
        comment: Some("x".repeat(chars)),
        ..numeric(name, 1.0)
    };
    client.score(with_comment("a", 150));
    client.score(with_comment("b", 150));
    client.score(with_comment("too-big", 1000));
    client.score(with_comment("c", 150));
    client.flush().await;

    let sent = batches(&server).await;
    let names: Vec<Vec<String>> = sent
        .iter()
        .map(|b| {
            b.iter()
                .map(|s| s["name"].as_str().unwrap().to_owned())
                .collect()
        })
        .collect();
    assert_eq!(names, [vec!["a", "b"], vec!["c"]]);
    for request in server.received_requests().await.unwrap() {
        assert!(request.body.len() <= 600, "{} bytes", request.body.len());
    }
}

#[tokio::test]
async fn server_errors_are_retried_because_scores_have_ids() {
    let (client, server) = setup(manual(), 1).await;
    Mock::given(method("POST"))
        .and(path(SCORES_PATH))
        .respond_with(json(500, json!({"message": "boom"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    accept_scores(&server).await;

    client.score(numeric("retried", 1.0));
    client.flush().await;
    let sent = batches(&server).await;
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[0][0]["id"], sent[1][0]["id"],
        "the same score IDs are resent"
    );
}

#[tokio::test]
async fn partial_rejections_are_logged_not_retried() {
    let (client, server) = setup(manual(), 2).await;
    Mock::given(method("POST"))
        .and(path(SCORES_PATH))
        .respond_with(json(
            207,
            json!({"accepted": 1, "rejected": 1, "errors": [{"message": "invalid value"}]}),
        ))
        .mount(&server)
        .await;

    client.score(numeric("ok", 1.0));
    client.score(numeric("bad", f64::MAX));
    client.flush().await;
    assert_eq!(batches(&server).await.len(), 1);
}

#[tokio::test]
async fn shutdown_sends_queued_scores_and_drops_later_ones() {
    let (client, server) = setup(manual(), 0).await;
    accept_scores(&server).await;

    client.score(numeric("before", 1.0));
    let clone = client.clone();
    clone.shutdown().await;
    client.score(numeric("after", 1.0));
    client.flush().await;

    let sent = batches(&server).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0][0]["name"], "before");
}

#[tokio::test]
async fn clones_share_one_queue() {
    let (client, server) = setup(manual(), 0).await;
    accept_scores(&server).await;
    let clone = client.clone();
    client.score(numeric("from-original", 1.0));
    clone.score(numeric("from-clone", 1.0));
    clone.flush().await;
    let sent = batches(&server).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].len(), 2);
}

#[test]
fn scores_outside_a_runtime_are_dropped_without_panicking() {
    let config = ClientConfig::builder("pk", "sk").build().unwrap();
    let client = LangfuseClient::new(config).unwrap();
    client.score(numeric("dropped", 1.0));
}

#[test]
fn invalid_queue_options_are_rejected() {
    let result = ClientConfig::builder("pk", "sk")
        .score_queue(queue(0, Duration::from_secs(1), 1000))
        .build();
    assert!(matches!(result, Err(LangfuseError::Config(_))));

    let capped = ClientConfig::builder("pk", "sk")
        .score_queue(queue(10, Duration::from_secs(1), 5_000_000))
        .build()
        .unwrap();
    assert_eq!(capped.score_queue().max_batch_bytes, 1_000_000);
}
