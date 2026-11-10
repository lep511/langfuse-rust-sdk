//! `LangfuseClient::run_batched_evaluation`: cursor paging, deduplication,
//! scores, limits, resume tokens and failures.
mod common;

use common::*;
use langfuse_rust_sdk::LangfuseError;
use langfuse_rust_sdk::batch_evaluation::{BatchEvaluationOptions, BatchEvaluationResumeToken};
use langfuse_rust_sdk::experiment::{
    CompositeEvaluatorInput, Evaluation, EvaluatorInput, composite_evaluator, evaluator,
};
use langfuse_rust_sdk::models::observations::ObservationV2;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const OBSERVATIONS: &str = "/api/public/v2/observations";

fn observation(id: &str, trace_id: Option<&str>, output: &str, updated_at: &str) -> Value {
    json!({
        "id": id,
        "traceId": trace_id,
        "projectId": "proj-1",
        "type": "GENERATION",
        "startTime": "2024-06-15T10:00:00Z",
        "updatedAt": updated_at,
        "environment": "production",
        "input": "question",
        "output": output
    })
}

fn page(data: Vec<Value>, cursor: Option<&str>) -> ResponseTemplate {
    json(200, json!({"data": data, "meta": {"cursor": cursor}}))
}

async fn mount_scores(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(202, json!({"accepted": 1})))
        .mount(server)
        .await;
}

async fn observation_requests(server: &MockServer) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == OBSERVATIONS)
        .collect()
}

async fn sent_scores(server: &MockServer) -> Vec<Value> {
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() == "/api/public/scores" {
            out.extend(body_json(&request).as_array().unwrap().iter().cloned());
        }
    }
    out
}

fn mapper() -> impl langfuse_rust_sdk::batch_evaluation::Mapper {
    |observation: ObservationV2| async move {
        EvaluatorInput::new(
            observation.input.unwrap_or_default(),
            observation.output.unwrap_or_default(),
        )
        .with_metadata(json!({"id": observation.id}))
    }
}

fn length() -> impl langfuse_rust_sdk::experiment::Evaluator {
    evaluator("length", |args: EvaluatorInput| async move {
        let text = args.output.as_str().unwrap_or_default();
        Evaluation::numeric("length", text.len() as f64)
            .with_metadata(&json!({"unit": "bytes", "run": "own"}))
    })
}

#[tokio::test]
async fn observations_are_paged_deduplicated_evaluated_and_scored() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param_is_missing("cursor"))
        .respond_with(page(
            vec![
                observation("obs-1", Some("t-1"), "old", "2024-06-15T10:00:01Z"),
                observation("obs-1", Some("t-1"), "newest", "2024-06-15T10:00:05Z"),
                observation("obs-2", None, "orphan", "2024-06-15T10:00:01Z"),
            ],
            Some("c2"),
        ))
        .expect(1)
        .mount(&server)
        .await;
    // An empty page does not end the run.
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param("cursor", "c2"))
        .respond_with(page(Vec::new(), Some("c3")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param("cursor", "c3"))
        .respond_with(page(
            vec![
                observation("obs-3", Some("t-3"), "fails", "2024-06-15T10:00:01Z"),
                observation("obs-4", Some("t-4"), "ok", "2024-06-15T10:00:01Z"),
            ],
            None,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let picky = evaluator("picky", |args: EvaluatorInput| async move {
        if args.output == "fails" {
            Err("cannot judge".to_string())
        } else {
            Ok(Evaluation::boolean("picky", true))
        }
    });
    let composite = composite_evaluator("overall", |args: CompositeEvaluatorInput| async move {
        Evaluation::numeric("overall", args.evaluations.len() as f64)
    });
    let filter = r#"[{"type":"string","column":"type","operator":"=","value":"GENERATION"}]"#;
    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_evaluator(length())
                .with_evaluator(picky)
                .with_composite_evaluator(composite)
                .with_filter(filter)
                .with_metadata("run", "nightly"),
        )
        .await
        .unwrap();

    assert!(result.completed);
    assert!(!result.has_more_items);
    assert!(result.resume_token.is_none());
    assert_eq!(result.total_items_fetched, 5);
    assert_eq!(result.total_items_processed, 3);
    assert_eq!(result.total_items_failed, 1);
    assert_eq!(result.failed_item_ids, ["obs-2"]);
    assert_eq!(result.error_summary.get("MissingTraceId"), Some(&1));
    assert_eq!(result.total_scores_created, 5);
    assert_eq!(result.total_composite_scores_created, 3);
    assert_eq!(result.total_evaluations_failed, 1);
    let stats: Vec<(&str, usize, usize, usize, usize)> = result
        .evaluator_stats
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.total_runs,
                s.successful_runs,
                s.failed_runs,
                s.total_scores_created,
            )
        })
        .collect();
    assert_eq!(stats, [("length", 3, 3, 0, 3), ("picky", 3, 2, 1, 2)]);
    // The most recently updated row of obs-1 was evaluated.
    let obs1 = &result.item_evaluations["obs-1"];
    assert_eq!(obs1[0].value.as_f64(), Some(6.0));
    assert_eq!(obs1.len(), 3);

    let requests = observation_requests(&server).await;
    assert_eq!(requests.len(), 3);
    assert_eq!(
        query_value(&requests[0], "fields"),
        "core,basic,io,metadata,time"
    );
    assert_eq!(query_value(&requests[0], "limit"), "50");
    let sent_filter: Value = serde_json::from_str(&query_value(&requests[0], "filter")).unwrap();
    assert_eq!(sent_filter, serde_json::from_str::<Value>(filter).unwrap());

    let scores = sent_scores(&server).await;
    assert_eq!(scores.len(), 8);
    let length_score = scores
        .iter()
        .find(|s| s["name"] == "length" && s["observationId"] == "obs-1")
        .unwrap();
    assert_eq!(length_score["traceId"], "t-1");
    assert_eq!(length_score["value"], 6.0);
    assert_eq!(length_score["environment"], "production");
    assert_eq!(
        length_score["metadata"],
        json!({"unit": "bytes", "run": "nightly"})
    );
    let composite_score = scores
        .iter()
        .find(|s| s["name"] == "overall" && s["observationId"] == "obs-3")
        .unwrap();
    assert_eq!(composite_score["value"], 1.0);
    assert_eq!(composite_score["metadata"], json!({"run": "nightly"}));

    let text = result.to_string();
    assert!(text.contains("Status: Completed"), "{text}");
    assert!(
        text.contains("Items fetched: 5\nItems processed: 3\nItems failed: 1\nSuccess rate: 60.0%"),
        "{text}"
    );
    assert!(
        text.contains(
            "  picky:\n    Runs: 2/3 (66.7% success)\n    Scores created: 2\n    Failed runs: 1"
        ),
        "{text}"
    );
    assert!(
        text.contains("Errors encountered:\n  MissingTraceId: 1"),
        "{text}"
    );
}

#[tokio::test]
async fn max_items_stops_with_a_token_that_resumes_after_the_last_page() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param_is_missing("cursor"))
        .respond_with(page(
            vec![
                observation("obs-1", Some("t"), "a", "2024-06-15T10:00:01Z"),
                observation("obs-2", Some("t"), "b", "2024-06-15T10:00:01Z"),
            ],
            Some("next"),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_evaluator(length())
                .with_fetch_batch_size(5)
                .with_max_items(2),
        )
        .await
        .unwrap();
    assert!(result.completed);
    assert!(result.has_more_items);
    assert_eq!(result.total_items_processed, 2);
    let requests = observation_requests(&server).await;
    assert_eq!(query_value(&requests[0], "limit"), "2");
    let token = result.resume_token.clone().unwrap();
    assert_eq!(token.cursor.as_deref(), Some("next"));
    assert_eq!(token.last_processed_id, "obs-2");
    assert_eq!(token.last_processed_timestamp, "2024-06-15T10:00:00Z");
    assert_eq!(token.items_processed, 2);
    assert_eq!(token.last_page_ids, ["obs-1", "obs-2"]);
    assert!(
        result
            .to_string()
            .contains("More items may be available beyond max_items limit")
    );

    // The token survives storage, and the next run continues at its cursor,
    // skipping a row of the last page that shows up again.
    let token: BatchEvaluationResumeToken =
        serde_json::from_str(&serde_json::to_string(&token).unwrap()).unwrap();
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param("cursor", "next"))
        .respond_with(page(
            vec![
                observation("obs-2", Some("t"), "b", "2024-06-15T10:00:01Z"),
                observation("obs-3", Some("t"), "c", "2024-06-15T10:00:01Z"),
            ],
            None,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let resumed = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_evaluator(length())
                .with_resume_from(token),
        )
        .await
        .unwrap();
    assert_eq!(resumed.total_items_processed, 1);
    assert!(resumed.item_evaluations.contains_key("obs-3"));
    assert!(resumed.resume_token.is_none());
}

#[tokio::test]
async fn a_failed_page_request_returns_an_incomplete_result_with_a_token() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param_is_missing("cursor"))
        .respond_with(page(
            vec![observation("obs-1", Some("t"), "a", "2024-06-15T10:00:01Z")],
            Some("c2"),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .and(query_param("cursor", "c2"))
        .respond_with(json(500, json!({"message": "boom"})))
        .expect(2)
        .mount(&server)
        .await;

    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_evaluator(length())
                .with_max_retries(1),
        )
        .await
        .unwrap();
    assert!(!result.completed);
    assert!(!result.has_more_items);
    assert_eq!(result.total_items_processed, 1);
    let token = result.resume_token.as_ref().unwrap();
    assert_eq!(token.cursor.as_deref(), Some("c2"));
    assert_eq!(token.items_processed, 1);
    let text = result.to_string();
    assert!(text.contains("Status: Incomplete"), "{text}");
    assert!(text.contains("Warning: Evaluation incomplete\n  Last processed: 2024-06-15T10:00:00Z\n  Items processed: 1"), "{text}");
}

#[tokio::test]
async fn a_token_without_cursor_bounds_the_start_time_and_skips_its_item() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .respond_with(page(
            vec![
                observation("obs-9", Some("t"), "seen", "2024-06-15T10:00:01Z"),
                observation("obs-8", Some("t"), "new", "2024-06-15T10:00:01Z"),
            ],
            None,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let token: BatchEvaluationResumeToken = serde_json::from_value(json!({
        "filter": "[]",
        "cursor": null,
        "last_processed_timestamp": "2024-06-15T10:00:00Z",
        "last_processed_id": "obs-9",
        "items_processed": 7
    }))
    .unwrap();

    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_evaluator(length())
                .with_resume_from(token),
        )
        .await
        .unwrap();
    assert_eq!(result.total_items_processed, 1);
    assert!(result.item_evaluations.contains_key("obs-8"));
    let request = &observation_requests(&server).await[0];
    let filter: Value = serde_json::from_str(&query_value(request, "filter")).unwrap();
    assert_eq!(
        filter,
        json!([{"type": "datetime", "column": "startTime", "operator": "<=", "value": "2024-06-15T10:00:00Z"}])
    );
}

#[tokio::test]
async fn mapper_errors_fail_the_item() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .respond_with(page(
            vec![observation("obs-1", Some("t"), "a", "2024-06-15T10:00:01Z")],
            None,
        ))
        .mount(&server)
        .await;
    let failing_mapper =
        |_observation: ObservationV2| async move { Err::<EvaluatorInput, _>("no input") };
    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(failing_mapper).with_evaluator(length()),
        )
        .await
        .unwrap();
    assert_eq!(result.total_items_failed, 1);
    assert_eq!(result.error_summary.get("MapperError"), Some(&1));
    assert!(sent_scores(&server).await.is_empty());
}

#[tokio::test]
async fn panicking_mappers_and_evaluators_do_not_stop_the_run() {
    let (client, server) = test_client().await;
    mount_scores(&server).await;
    Mock::given(method("GET"))
        .and(path(OBSERVATIONS))
        .respond_with(page(
            vec![
                observation("obs-1", Some("t"), "panic", "2024-06-15T10:00:01Z"),
                observation("obs-2", Some("t"), "ok", "2024-06-15T10:00:01Z"),
                observation(
                    "obs-3",
                    Some("t"),
                    "evaluator-panic",
                    "2024-06-15T10:00:01Z",
                ),
            ],
            None,
        ))
        .mount(&server)
        .await;
    let panicking_mapper = |observation: ObservationV2| async move {
        let output = observation.output.unwrap_or_default();
        assert_ne!(output, json!("panic"), "mapper bug");
        EvaluatorInput::new(observation.input.unwrap_or_default(), output)
    };
    let fragile = evaluator("fragile", |args: EvaluatorInput| async move {
        if args.output == json!("evaluator-panic") {
            panic!("evaluator bug");
        }
        Evaluation::numeric("fragile", 1.0)
    });
    let result = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(panicking_mapper)
                .with_evaluator(length())
                .with_evaluator(fragile),
        )
        .await
        .unwrap();

    assert!(result.completed);
    assert_eq!(result.total_items_fetched, 3);
    assert_eq!(result.total_items_failed, 1);
    assert_eq!(result.failed_item_ids, ["obs-1"]);
    assert_eq!(result.error_summary.get("MapperError"), Some(&1));
    let fragile_stats = result
        .evaluator_stats
        .iter()
        .find(|s| s.name == "fragile")
        .unwrap();
    assert_eq!(fragile_stats.successful_runs, 1);
    assert_eq!(fragile_stats.failed_runs, 1);
    // length on obs-2 and obs-3, fragile on obs-2; flushed before returning.
    assert_eq!(result.total_scores_created, 3);
    assert_eq!(sent_scores(&server).await.len(), 3);
}

#[tokio::test]
async fn invalid_filters_and_mismatched_tokens_are_rejected() {
    let (client, server) = test_client().await;
    for filter in ["{\"type\": \"string\"}", "not json"] {
        let err = client
            .run_batched_evaluation(BatchEvaluationOptions::new(mapper()).with_filter(filter))
            .await
            .unwrap_err();
        assert!(matches!(err, LangfuseError::Config(_)), "{err:?}");
    }
    let token: BatchEvaluationResumeToken = serde_json::from_value(json!({
        "filter": "[]", "cursor": "c", "last_processed_timestamp": "", "last_processed_id": "", "items_processed": 0
    }))
    .unwrap();
    let err = client
        .run_batched_evaluation(
            BatchEvaluationOptions::new(mapper())
                .with_filter(r#"[{"type":"string","column":"name","operator":"=","value":"x"}]"#)
                .with_resume_from(token),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("different filter"), "{err}");
    let err = client
        .run_batched_evaluation(BatchEvaluationOptions::new(mapper()).with_fetch_batch_size(0))
        .await
        .unwrap_err();
    assert!(matches!(err, LangfuseError::Config(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}
