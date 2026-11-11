//! Score observations that are already stored in Langfuse with
//! `run_batched_evaluation`, the counterpart of the Python SDK method of the
//! same name: it pages through the observations matching a filter, runs your
//! evaluators on each one, and writes the results back as scores.
//!
//! To have something to evaluate, the example first ingests one trace with
//! three generations (through the OTLP endpoint, no `otel` feature needed)
//! and waits until they can be queried. It then evaluates them (one evaluator
//! fails on purpose for the empty answer), prints the summary, and deletes
//! the scores and the trace. Point the filter (and its `startTime` window) at
//! your own observations to evaluate them.
//!
//! **Writes data**, then deletes it: one trace and eight scores. The cleanup
//! makes nine delete requests, which count against the trace-data-deletion
//! rate limit of Langfuse Cloud (50 requests per day on the Hobby plan).
//! Expect the run to take about a minute, most of it waiting for ingestion.
//!
//! **Docs:** <https://langfuse.com/docs/evaluation/overview>; the Python
//! method is described in
//! <https://langfuse.com/docs/observability/sdk/upgrade-path/python-v4-to-v5>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example batch_evaluation
//! ```

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{SecondsFormat, Utc};
use langfuse_rust_sdk::api::observations::ListObservationsParams;
use langfuse_rust_sdk::api::scores::ListScoresParams;
use langfuse_rust_sdk::batch_evaluation::BatchEvaluationOptions;
use langfuse_rust_sdk::experiment::{
    CompositeEvaluatorInput, Evaluation, EvaluatorInput, composite_evaluator, evaluator,
};
use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
use langfuse_rust_sdk::models::ingestion::*;
use langfuse_rust_sdk::models::observations::ObservationV2;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};
use serde_json::json;

/// (question, answer) of the three generations; the last answer is empty.
const ANSWERS: [(&str, &str); 3] = [
    ("What is Rust?", "A systems programming language."),
    (
        "What is Langfuse?",
        "An open-source LLM engineering platform.",
    ),
    ("What is tokio?", ""),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = LangfuseClient::new(ClientConfig::from_env()?)?;

    // The observations of this run start after `since`, which bounds every
    // v2 observations query below (without a lower bound the API searches
    // from 2020-01-01).
    let since =
        (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let trace_id = create_trace_id(None);
    ingest_generations(&client, &trace_id).await?;
    println!("Ingested trace {trace_id}; waiting until its observations can be queried...");
    wait_for_observations(&client, &trace_id, &since, ANSWERS.len()).await?;

    // The mapper turns a stored observation into what the evaluators see.
    // The v2 API returns `input` and `output` as raw strings, so structured
    // payloads would have to be parsed here.
    let mapper = |observation: ObservationV2| async move {
        EvaluatorInput::new(
            observation.input.unwrap_or_default(),
            observation.output.unwrap_or_default(),
        )
    };
    // Evaluators are the same as in experiments (see `run_experiment`): they
    // return an `Evaluation`, or an error, which is counted as a failed run
    // of that evaluator and creates no score.
    let not_empty = evaluator("not_empty", |args: EvaluatorInput| async move {
        let text = args.output.as_str().unwrap_or_default().trim().to_owned();
        Evaluation::boolean("not_empty", !text.is_empty())
    });
    let conciseness = evaluator("conciseness", |args: EvaluatorInput| async move {
        let words = args
            .output
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .count();
        if words == 0 {
            return Err("nothing to measure".to_string());
        }
        Ok(Evaluation::numeric("conciseness", 1.0 / words as f64)
            .with_comment(format!("{words} words")))
    });
    // Combines the evaluations of one observation into one more score.
    let quality = composite_evaluator("quality", |args: CompositeEvaluatorInput| async move {
        let passed = args
            .evaluations
            .iter()
            .all(|e| e.value.as_f64().unwrap_or(0.0) > 0.0);
        Evaluation::boolean("quality", passed)
    });

    // Which observations to evaluate: a v2 filter, a JSON array of
    // conditions. The run sends no `fromStartTime`: its time window is the
    // `startTime` condition of the filter, which keeps the query bounded.
    let filter = json!([
        {"type": "string", "column": "traceId", "operator": "=", "value": trace_id},
        {"type": "datetime", "column": "startTime", "operator": ">=", "value": since},
    ]);
    // `max_items` caps the run; a run that stops there (or on an error)
    // returns a resume token to continue with `with_resume_from`. The
    // metadata is added to every score. `verbose` logs the progress.
    let options = BatchEvaluationOptions::new(mapper)
        .with_evaluator(not_empty)
        .with_evaluator(conciseness)
        .with_composite_evaluator(quality)
        .with_filter(filter.to_string())
        .with_metadata("evaluation_run", "rust-sdk-example")
        .with_max_items(100)
        .with_verbose(true);
    let result = client.run_batched_evaluation(options).await?;
    println!("{result}");
    for (observation_id, evaluations) in &result.item_evaluations {
        let scores: Vec<String> = evaluations
            .iter()
            .map(|e| format!("{}={:?}", e.name, e.value))
            .collect();
        println!("  {observation_id}: {}", scores.join(", "));
    }

    // Clean up: scores are ingested asynchronously, so wait until all of
    // them can be listed. Deleting the trace would remove its scores as well,
    // but asynchronously (usually within 15 minutes); deleting them one by one
    // makes them disappear right away, at the price of one delete request
    // per score.
    let expected: usize = result.item_evaluations.values().map(Vec::len).sum();
    let params = ListScoresParams {
        trace_id: Some(trace_id.clone()),
        ..Default::default()
    };
    let mut scores = Vec::new();
    for _ in 0..20 {
        scores = client.scores.list(&params).await?.data;
        if scores.len() >= expected {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    for score in &scores {
        client.scores.delete(&score.base().id).await?;
    }
    client.traces.delete(&trace_id).await?;
    println!("\nDeleted {} scores and the trace.", scores.len());
    client.shutdown().await;
    Ok(())
}

/// Ingest one trace with a generation per question, as hand-built OTLP spans
/// (see the `otel_ingestion` example for the attributes).
async fn ingest_generations(
    client: &LangfuseClient,
    trace_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let spans = ANSWERS
        .iter()
        .enumerate()
        .map(|(i, (question, answer))| {
            let start = now + i as u128 * 1_000_000_000;
            Ok(OtelSpan {
                attributes: Some(vec![
                    OtelKeyValue::string("langfuse.observation.type", "generation"),
                    OtelKeyValue::string(
                        "langfuse.trace.name",
                        "rust-sdk-batch-evaluation-example",
                    ),
                    OtelKeyValue::string("langfuse.observation.input", *question),
                    OtelKeyValue::string("langfuse.observation.output", *answer),
                    OtelKeyValue::string("langfuse.observation.model.name", "example-model"),
                ]),
                ..OtelSpan::new(
                    trace_id,
                    create_observation_id(None),
                    format!("answer-{i}"),
                    start.to_string(),
                    (start + 500_000_000).to_string(),
                )
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let request = IngestTracesRequest {
        resource_spans: vec![OtelResourceSpan {
            resource: None,
            scope_spans: vec![OtelScopeSpan {
                scope: Some(OtelInstrumentationScope {
                    name: Some("rust-sdk-example".into()),
                    version: None,
                    attributes: None,
                }),
                spans,
            }],
        }],
    };
    client.ingestion.ingest_traces(&request).await?;
    Ok(())
}

/// Poll the v2 observations API until the trace has `count` observations
/// that started at or after `since`. Ingestion is asynchronous: new data can
/// usually be queried after 15 to 30 seconds.
async fn wait_for_observations(
    client: &LangfuseClient,
    trace_id: &str,
    since: &str,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..30 {
        let page = client
            .observations
            .list(&ListObservationsParams {
                trace_id: Some(trace_id.to_owned()),
                from_start_time: Some(since.to_owned()),
                ..Default::default()
            })
            .await?;
        if page.data.len() >= count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err("observations did not show up within a minute".into())
}
