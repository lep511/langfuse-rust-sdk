//! `LangfuseTracer::run_experiment` (feature `otel`): span tree, experiment
//! attributes, scores and failure handling, checked on the exported OTLP bodies.
#![cfg(feature = "otel")]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::*;
use langfuse_rust_sdk::api::datasets::GetDatasetOptions;
use langfuse_rust_sdk::experiment::{
    CompositeEvaluatorInput, Evaluation, EvaluatorInput, ExperimentItem, ExperimentOptions,
    LocalExperimentItem, RunEvaluatorInput, composite_evaluator, evaluator, run_evaluator,
};
use langfuse_rust_sdk::ids::create_observation_id;
use langfuse_rust_sdk::otel::{LangfuseTracer, ObservationType, TracingOptions};
use langfuse_rust_sdk::{LangfuseClient, LangfuseError};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

const OTEL_PATH: &str = "/api/public/otel/v1/traces";

/// Tracer that only exports on `flush`, with the Langfuse endpoints mocked.
async fn setup(options: TracingOptions) -> (LangfuseClient, LangfuseTracer, MockServer) {
    let (client, server) = test_client().await;
    Mock::given(method("POST"))
        .and(path(OTEL_PATH))
        .respond_with(json(200, json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/public/scores"))
        .respond_with(json(202, json!({"accepted": 1})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/projects"))
        .respond_with(json(
            200,
            json!({"data": [{"id": "proj-1", "name": "p", "organization": {"id": "o", "name": "o"}, "metadata": {}}]}),
        ))
        .mount(&server)
        .await;
    let options = options.with_flush_interval(Duration::from_secs(3600));
    let tracer = client.tracer(options).unwrap();
    (client, tracer, server)
}

#[derive(Debug, Clone)]
struct Span(Value);

impl Span {
    fn name(&self) -> &str {
        self.0["name"].as_str().unwrap()
    }
    fn id(&self) -> &str {
        self.0["spanId"].as_str().unwrap()
    }
    fn trace_id(&self) -> &str {
        self.0["traceId"].as_str().unwrap()
    }
    fn parent_id(&self) -> Option<&str> {
        self.0["parentSpanId"].as_str()
    }
    fn attr(&self, key: &str) -> Option<Value> {
        let matches: Vec<&Value> = self.0["attributes"]
            .as_array()?
            .iter()
            .filter(|kv| kv["key"] == key)
            .collect();
        assert!(matches.len() <= 1, "attribute {key} exported twice");
        let value = &matches.first()?["value"];
        ["stringValue", "intValue", "boolValue", "doubleValue"]
            .iter()
            .find_map(|k| value.get(*k).cloned())
    }
    fn attr_str(&self, key: &str) -> Option<String> {
        self.attr(key).and_then(|v| v.as_str().map(str::to_owned))
    }
    fn attr_json(&self, key: &str) -> Option<Value> {
        self.attr_str(key)
            .map(|s| serde_json::from_str(&s).unwrap())
    }
}

async fn exported(server: &MockServer) -> Vec<Span> {
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() != OTEL_PATH {
            continue;
        }
        let body: Value = body_json(&request);
        for resource in body["resourceSpans"].as_array().unwrap() {
            for scope in resource["scopeSpans"].as_array().unwrap() {
                out.extend(scope["spans"].as_array().unwrap().iter().cloned().map(Span));
            }
        }
    }
    out
}

async fn sent_scores(server: &MockServer) -> Vec<Value> {
    let mut out = Vec::new();
    for request in server.received_requests().await.unwrap() {
        if request.url.path() == "/api/public/scores" {
            let body: Value = body_json(&request);
            out.extend(body.as_array().unwrap().iter().cloned());
        }
    }
    out
}

fn named<'a>(spans: &'a [Span], name: &str) -> Vec<&'a Span> {
    spans.iter().filter(|s| s.name() == name).collect()
}

fn in_trace<'a>(spans: &'a [Span], trace_id: &str, name: &str) -> &'a Span {
    let found: Vec<&Span> = spans
        .iter()
        .filter(|s| s.trace_id() == trace_id && s.name() == name)
        .collect();
    assert_eq!(found.len(), 1, "one {name} span in trace {trace_id}");
    found[0]
}

fn exact_match() -> impl langfuse_rust_sdk::experiment::Evaluator {
    evaluator("exact_match", |args: EvaluatorInput| async move {
        Evaluation::boolean("exact_match", args.expected_output == Some(args.output))
            .with_comment("compared")
    })
}

#[tokio::test]
async fn local_experiments_record_the_item_tree_attributes_and_scores() {
    let (_client, tracer, server) =
        setup(TracingOptions::default().with_environment("production")).await;
    let data = vec![
        LocalExperimentItem::new(json!({"country": "France"}))
            .with_expected_output("Paris")
            .with_metadata(json!({"difficulty": "easy", "experiment_name": "overridden"})),
        LocalExperimentItem::new(json!({"country": "Italy"})).with_expected_output("Rome"),
    ];
    let task_tracer = tracer.clone();
    let task = move |item: ExperimentItem| {
        let tracer = task_tracer.clone();
        async move {
            let generation = tracer.start_observation("llm", ObservationType::Generation);
            generation.set_model("gpt-test");
            generation.end();
            let country = item.input().unwrap()["country"]
                .as_str()
                .unwrap()
                .to_owned();
            Ok::<_, String>(if country == "France" {
                "Paris"
            } else {
                "Milan"
            })
        }
    };
    let failing = evaluator("judge", |_args: EvaluatorInput| async move {
        Err::<Evaluation, _>("judge unavailable")
    });
    let composite = composite_evaluator("overall", |args: CompositeEvaluatorInput| async move {
        Evaluation::numeric("overall", args.evaluations.len() as f64)
    });
    let accuracy = run_evaluator("accuracy", |run: RunEvaluatorInput| async move {
        let passed = run
            .item_results
            .iter()
            .flat_map(|r| &r.evaluations)
            .filter(|e| e.name == "exact_match")
            .filter_map(|e| e.value.as_f64())
            .sum::<f64>();
        Evaluation::numeric("accuracy", passed / run.item_results.len() as f64)
    });
    let options = ExperimentOptions::new("capitals")
        .with_run_name("run-1")
        .with_description("Country to capital")
        .with_metadata("model", "gpt-test")
        .with_evaluator(exact_match())
        .with_evaluator(failing)
        .with_composite_evaluator(composite)
        .with_run_evaluator(accuracy);

    let result = tracer.run_experiment(data, task, options).await.unwrap();

    // Result.
    assert_eq!(result.name, "capitals");
    assert_eq!(result.run_name, "run-1");
    assert_eq!(result.item_results.len(), 2);
    assert_eq!(result.experiment_id.len(), 16);
    assert_eq!(
        result.experiment_url.as_deref(),
        Some(
            format!(
                "{}/project/proj-1/experiments/results?baseline={}",
                server.uri(),
                result.experiment_id
            )
            .as_str()
        )
    );
    let first = &result.item_results[0];
    assert_eq!(first.output, json!("Paris"));
    let names: Vec<&str> = first.evaluations.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["exact_match", "overall"]);
    assert_eq!(first.evaluations[1].value.as_f64(), Some(1.0));
    assert_eq!(result.average("exact_match"), Some(0.5));
    assert_eq!(result.run_evaluations.len(), 1);
    assert_eq!(result.run_evaluations[0].value.as_f64(), Some(0.5));

    // Span tree of the first item.
    let spans = exported(&server).await;
    assert_eq!(named(&spans, "experiment-item-run").len(), 2);
    let trace_id = first.trace_id.as_deref().unwrap();
    let run = in_trace(&spans, trace_id, "experiment-item-run");
    let task = in_trace(&spans, trace_id, "experiment-item-task");
    let generation = in_trace(&spans, trace_id, "llm");
    let evaluation = in_trace(&spans, trace_id, "experiment-item-evaluation");
    let exact = in_trace(&spans, trace_id, "exact_match");
    let judge = in_trace(&spans, trace_id, "judge");
    let overall = in_trace(&spans, trace_id, "overall");
    assert_eq!(run.parent_id(), None);
    assert_eq!(task.parent_id(), Some(run.id()));
    assert_eq!(generation.parent_id(), Some(task.id()));
    assert_eq!(evaluation.parent_id(), Some(run.id()));
    for span in [exact, judge, overall] {
        assert_eq!(span.parent_id(), Some(evaluation.id()));
        assert_eq!(
            span.attr_str("langfuse.observation.type").as_deref(),
            Some("evaluator")
        );
    }

    // Experiment attributes: on every span, with the experiment environment.
    let item_id = create_observation_id(Some(r#"{"country": "France"}"#));
    for span in [run, task, generation, evaluation, exact, judge, overall] {
        assert_eq!(
            span.attr_str("langfuse.environment").as_deref(),
            Some("sdk-experiment"),
            "{}",
            span.name()
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.id").as_deref(),
            Some(result.experiment_id.as_str())
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.name").as_deref(),
            Some("run-1")
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.item.id").as_deref(),
            Some(item_id.as_str())
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.item.root_observation_id")
                .as_deref(),
            Some(task.id())
        );
        assert_eq!(
            span.attr_json("langfuse.experiment.metadata"),
            Some(json!({"model": "gpt-test"}))
        );
        assert_eq!(
            span.attr_json("langfuse.experiment.item.metadata"),
            Some(json!({"difficulty": "easy", "experiment_name": "overridden"}))
        );
        assert!(span.attr("langfuse.experiment.dataset.id").is_none());
    }
    for span in [run, task] {
        assert_eq!(
            span.attr_str("langfuse.experiment.description").as_deref(),
            Some("Country to capital")
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.item.expected_output")
                .as_deref(),
            Some("Paris")
        );
        assert_eq!(
            span.attr_json("langfuse.observation.input"),
            Some(json!({"country": "France"}))
        );
        assert_eq!(
            span.attr_str("langfuse.observation.output").as_deref(),
            Some("Paris")
        );
        // Item metadata, run metadata, then the experiment keys, which win.
        let metadata = |key: &str| span.attr_json(&format!("langfuse.observation.metadata.{key}"));
        assert_eq!(metadata("experiment_name"), Some(json!("capitals")));
        assert_eq!(metadata("experiment_run_name"), Some(json!("run-1")));
        assert_eq!(metadata("difficulty"), Some(json!("easy")));
        assert_eq!(metadata("model"), Some(json!("gpt-test")));
        assert!(metadata("dataset_id").is_none());
    }

    // Evaluator spans.
    assert_eq!(
        exact.attr_json("langfuse.observation.input"),
        Some(json!({
            "input": {"country": "France"},
            "output": "Paris",
            "expected_output": "Paris",
            "metadata": {"difficulty": "easy", "experiment_name": "overridden"}
        }))
    );
    assert_eq!(
        exact.attr_json("langfuse.observation.metadata.evaluator_kind"),
        Some(json!("item"))
    );
    assert_eq!(
        judge.attr_json("langfuse.observation.metadata.evaluator_index"),
        Some(json!(1))
    );
    assert_eq!(
        exact.attr_json("langfuse.observation.output"),
        Some(
            json!([{"name": "exact_match", "value": true, "comment": "compared", "data_type": "BOOLEAN"}])
        )
    );
    assert_eq!(
        judge.attr_str("langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert_eq!(
        judge.attr_json("langfuse.observation.output"),
        Some(json!({"error": "judge unavailable"}))
    );
    assert_eq!(judge.0["status"]["code"], 2);
    assert_eq!(
        overall.attr_json("langfuse.observation.metadata.evaluator_kind"),
        Some(json!("composite"))
    );
    assert_eq!(
        overall.attr_json("langfuse.observation.input").unwrap()["evaluations"][0]["name"],
        "exact_match"
    );
    assert_eq!(
        evaluation.attr_json("langfuse.observation.output"),
        Some(json!({
            "evaluator_count": 3,
            "evaluation_count": 2,
            "failed_evaluator_count": 1,
            "skipped_evaluator_count": 0
        }))
    );

    // Scores: item scores on the task span, the run score on the experiment,
    // with the tracer's environment.
    let scores = sent_scores(&server).await;
    assert_eq!(scores.len(), 5);
    let item_scores: Vec<&Value> = scores.iter().filter(|s| s["traceId"] == trace_id).collect();
    assert_eq!(item_scores.len(), 2);
    let exact_score = item_scores
        .iter()
        .find(|s| s["name"] == "exact_match")
        .unwrap();
    assert_eq!(exact_score["observationId"], task.id());
    assert_eq!(exact_score["value"], 1.0);
    assert_eq!(exact_score["dataType"], "BOOLEAN");
    assert_eq!(exact_score["comment"], "compared");
    assert_eq!(exact_score["environment"], "production");
    let run_score = scores.iter().find(|s| s["name"] == "accuracy").unwrap();
    assert_eq!(run_score["datasetRunId"], json!(result.experiment_id));
    assert!(run_score.get("traceId").is_none());
    assert_eq!(run_score["environment"], "production");
}

#[tokio::test]
async fn dataset_experiments_derive_their_id_and_link_the_items() {
    let (client, tracer, server) = setup(TracingOptions::default()).await;
    Mock::given(method("GET"))
        .and(path("/api/public/v2/datasets/capitals"))
        .respond_with(json(
            200,
            json!({"id": "ds-1", "name": "capitals", "projectId": "proj-1", "createdAt": TS, "updatedAt": TS}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/dataset-items"))
        .respond_with(json(
            200,
            json!({
                "data": [{
                    "id": "item-1", "status": "ACTIVE", "input": "France", "expectedOutput": {"city": "Paris"},
                    "metadata": {"region": "eu"}, "datasetId": "ds-1", "datasetName": "capitals",
                    "createdAt": TS, "updatedAt": TS
                }],
                "meta": {"page": 1, "limit": 50, "totalItems": 1, "totalPages": 1}
            }),
        ))
        .mount(&server)
        .await;
    let version = "2024-06-15T10:30:00.250Z".parse().unwrap();
    let dataset = client
        .get_dataset(
            "capitals",
            &GetDatasetOptions::default().with_version(version),
        )
        .await
        .unwrap();

    let result = dataset
        .run_experiment(
            &tracer,
            |item: ExperimentItem| async move {
                let input = item.input().cloned();
                Ok::<_, String>(json!({"city": "Paris", "asked": input}))
            },
            ExperimentOptions::new("capitals").with_run_name("nightly"),
        )
        .await
        .unwrap();

    // sha256('["langfuse-experiment-v1","proj-1","ds-1","nightly"]')[:16]
    let expected_id = create_observation_id(Some(
        r#"["langfuse-experiment-v1","proj-1","ds-1","nightly"]"#,
    ));
    assert_eq!(result.experiment_id, expected_id);
    let spans = exported(&server).await;
    let run = &named(&spans, "experiment-item-run")[0];
    let task = &named(&spans, "experiment-item-task")[0];
    for span in [run, task] {
        assert_eq!(
            span.attr_str("langfuse.experiment.dataset.id").as_deref(),
            Some("ds-1")
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.item.id").as_deref(),
            Some("item-1")
        );
        assert_eq!(
            span.attr_str("langfuse.experiment.item.version").as_deref(),
            Some("2024-06-15T10:30:00.250000Z")
        );
        assert_eq!(
            span.attr_json("langfuse.experiment.item.expected_output"),
            Some(json!({"city": "Paris"}))
        );
        assert_eq!(
            span.attr_json("langfuse.observation.metadata.dataset_item_id"),
            Some(json!("item-1"))
        );
        assert_eq!(
            span.attr_json("langfuse.observation.metadata.region"),
            Some(json!("eu"))
        );
    }
    // No evaluators: no evaluation span.
    assert!(named(&spans, "experiment-item-evaluation").is_empty());
}

#[tokio::test]
async fn failed_items_are_marked_as_errors_and_left_out() {
    let (_client, tracer, server) = setup(TracingOptions::default()).await;
    let data = vec![
        LocalExperimentItem::new(Value::Null),
        LocalExperimentItem::new("boom"),
        LocalExperimentItem::new("ok"),
    ];
    let result = tracer
        .run_experiment(
            data,
            |item: ExperimentItem| async move {
                match item.input().and_then(Value::as_str) {
                    Some("boom") => Err("model timeout".to_string()),
                    other => Ok(other.unwrap_or_default().to_uppercase()),
                }
            },
            ExperimentOptions::new("failures").with_evaluator(exact_match()),
        )
        .await
        .unwrap();
    assert_eq!(result.item_results.len(), 1);
    assert_eq!(result.item_results[0].output, json!("OK"));

    let spans = exported(&server).await;
    let runs = named(&spans, "experiment-item-run");
    assert_eq!(runs.len(), 3);
    let missing = runs
        .iter()
        .find(|s| s.attr("langfuse.observation.input").is_none())
        .unwrap();
    assert_eq!(
        missing.attr_str("langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    assert_eq!(
        missing.attr_str("langfuse.observation.output").as_deref(),
        Some("Error: Experiment Item is missing input. Skipping item.")
    );
    let failed_task = named(&spans, "experiment-item-task")
        .into_iter()
        .find(|s| s.attr_str("langfuse.observation.input").as_deref() == Some("boom"))
        .unwrap();
    assert_eq!(
        failed_task
            .attr_str("langfuse.observation.output")
            .as_deref(),
        Some("Error: model timeout")
    );
    assert_eq!(
        failed_task
            .attr_str("langfuse.observation.status_message")
            .as_deref(),
        Some("model timeout")
    );
    let failed_run = runs
        .iter()
        .find(|s| s.trace_id() == failed_task.trace_id())
        .unwrap();
    assert_eq!(
        failed_run.attr_str("langfuse.observation.level").as_deref(),
        Some("ERROR")
    );
    // Only the successful item was evaluated.
    assert_eq!(named(&spans, "experiment-item-evaluation").len(), 1);
    assert_eq!(sent_scores(&server).await.len(), 1);
}

#[tokio::test]
async fn the_composite_evaluator_is_skipped_without_evaluations() {
    let (_client, tracer, server) = setup(TracingOptions::default()).await;
    let nothing = evaluator("nothing", |_args: EvaluatorInput| async move {
        None::<Evaluation>
    });
    let composite = composite_evaluator("overall", |_args: CompositeEvaluatorInput| async move {
        Evaluation::numeric("overall", 1.0)
    });
    let result = tracer
        .run_experiment(
            vec![LocalExperimentItem::new("x")],
            |_item: ExperimentItem| async move { Ok::<_, String>("y") },
            ExperimentOptions::new("skip")
                .with_evaluator(nothing)
                .with_composite_evaluator(composite),
        )
        .await
        .unwrap();
    assert!(result.item_results[0].evaluations.is_empty());
    let spans = exported(&server).await;
    assert!(named(&spans, "overall").is_empty());
    assert_eq!(
        named(&spans, "experiment-item-evaluation")[0].attr_json("langfuse.observation.output"),
        Some(json!({
            "evaluator_count": 2,
            "evaluation_count": 0,
            "failed_evaluator_count": 0,
            "skipped_evaluator_count": 1
        }))
    );
}

#[tokio::test]
async fn items_run_concurrently_and_results_keep_the_data_order() {
    let (_client, tracer, _server) = setup(TracingOptions::default()).await;
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let data: Vec<LocalExperimentItem> = (0..6).map(LocalExperimentItem::new).collect();
    let (running_task, peak_task) = (Arc::clone(&running), Arc::clone(&peak));
    let task = move |item: ExperimentItem| {
        let (running, peak) = (Arc::clone(&running_task), Arc::clone(&peak_task));
        async move {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            let index = item.input().unwrap().as_u64().unwrap();
            // Later items finish first.
            tokio::time::sleep(Duration::from_millis(30 - 5 * index)).await;
            running.fetch_sub(1, Ordering::SeqCst);
            Ok::<_, String>(index * 10)
        }
    };
    let result = tracer
        .run_experiment(
            data,
            task,
            ExperimentOptions::new("order").with_max_concurrency(3),
        )
        .await
        .unwrap();
    let outputs: Vec<u64> = result
        .item_results
        .iter()
        .map(|r| r.output.as_u64().unwrap())
        .collect();
    assert_eq!(outputs, [0, 10, 20, 30, 40, 50]);
    assert_eq!(peak.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn an_explicit_experiment_id_is_used_and_validated() {
    let (_client, tracer, _server) = setup(TracingOptions::default()).await;
    let task = |_item: ExperimentItem| async move { Ok::<_, String>("y") };
    let result = tracer
        .run_experiment(
            vec![LocalExperimentItem::new("x")],
            task,
            ExperimentOptions::new("fixed").with_experiment_id("exp-42"),
        )
        .await
        .unwrap();
    assert_eq!(result.experiment_id, "exp-42");
    assert_eq!(result.item_results[0].experiment_id, "exp-42");
    assert!(
        result.run_name.starts_with("fixed - "),
        "{}",
        result.run_name
    );
    assert!(result.run_name.ends_with('Z'), "{}", result.run_name);

    for options in [
        ExperimentOptions::new("bad").with_experiment_id("  "),
        ExperimentOptions::new("bad").with_max_concurrency(0),
    ] {
        let err = tracer
            .run_experiment(vec![LocalExperimentItem::new("x")], task, options)
            .await
            .unwrap_err();
        assert!(matches!(err, LangfuseError::Config(_)), "{err:?}");
    }
}

#[tokio::test]
async fn with_tracing_disabled_tasks_still_run_without_recording() {
    let (_client, tracer, server) = setup(TracingOptions::default().with_enabled(false)).await;
    let result = tracer
        .run_experiment(
            vec![LocalExperimentItem::new("x").with_expected_output("y")],
            |_item: ExperimentItem| async move { Ok::<_, String>("y") },
            ExperimentOptions::new("off").with_evaluator(exact_match()),
        )
        .await
        .unwrap();
    assert_eq!(result.item_results.len(), 1);
    assert_eq!(result.item_results[0].evaluations.len(), 1);
    assert!(result.item_results[0].trace_id.is_none());
    assert!(result.experiment_url.is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn experiment_and_evaluation_futures_can_be_spawned() {
    fn assert_send<T: Send>(_: &T) {}
    let (client, tracer, _server) = setup(TracingOptions::default()).await;
    let shared = String::from("borrowed by the task");
    let task = |_item: ExperimentItem| async { Ok::<_, String>(shared.len()) };
    let experiment = tracer.run_experiment(
        vec![LocalExperimentItem::new("x")],
        task,
        ExperimentOptions::new("send").with_evaluator(exact_match()),
    );
    assert_send(&experiment);
    let mapper = |o: langfuse_rust_sdk::models::observations::ObservationV2| async move {
        EvaluatorInput::new(o.input.unwrap_or_default(), o.output.unwrap_or_default())
    };
    let batch = client.run_batched_evaluation(
        langfuse_rust_sdk::batch_evaluation::BatchEvaluationOptions::new(mapper),
    );
    assert_send(&batch);
    assert_send(&client.get_dataset("d", &GetDatasetOptions::default()));
    drop((experiment, batch));
}
