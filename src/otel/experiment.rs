//! [`LangfuseTracer::run_experiment`]: run a task on every experiment item,
//! trace it, evaluate it and score it.
//!
//! Langfuse builds experiments from the traces alone: every span of an item
//! carries the `langfuse.experiment.*` attributes and the `sdk-experiment`
//! environment, and the scores reference the task span (item scores) or the
//! experiment ID (run scores).

use std::io;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use futures_util::stream;
use opentelemetry::context::FutureExt;
use opentelemetry::{Context, KeyValue};
use serde::Serialize;
use serde_json::{Map, Value, json};

use super::attributes::{self as attr, js_safe};
use super::observation::{Level, Observation, ObservationType};
use super::propagation::{ExperimentAttributes, experiment_context, inherited_attributes};
use super::tracer::LangfuseTracer;
use crate::error::LangfuseError;
use crate::experiment::{
    CompositeEvaluator, CompositeEvaluatorInput, Evaluation, Evaluator, EvaluatorInput,
    ExperimentItem, ExperimentItemResult, ExperimentOptions, ExperimentResult, RunEvaluatorInput,
    Task, catch_panic, python_float_repr, python_isoformat,
};
use crate::http::encode_path_segment;
use crate::ids::{create_observation_id, seeded_hex};
use crate::models::datasets::DatasetWithItems;
use crate::models::scores::CreateScoreRequest;

impl LangfuseTracer {
    /// Run an experiment: the task on every item of `data` (at most
    /// [`max_concurrency`](ExperimentOptions::max_concurrency) at a time),
    /// then the evaluators on each output, then the run evaluators, and
    /// return the results. Spans and scores are flushed before returning.
    ///
    /// Each item is traced as an `experiment-item-run` span with two
    /// children: `experiment-item-task`, in whose scope the task runs (its
    /// observations become children), and `experiment-item-evaluation`, with
    /// one `evaluator` span per evaluator. Item evaluations are scored on the
    /// task span, run evaluations on the experiment. Every span of the item
    /// gets the `sdk-experiment` environment; scores get the tracer's.
    ///
    /// Items run concurrently on the calling task (like `join_all`, not on
    /// other threads): move CPU-heavy work to `tokio::task::spawn_blocking`.
    ///
    /// Failures do not stop the run: an item without input or whose task
    /// fails (returns an error or panics) is logged, marked as an error and
    /// left out of the results; a failing (or panicking) evaluator is recorded
    /// on its span and the others still run.
    ///
    /// The experiment ID is [`experiment_id`](ExperimentOptions::experiment_id)
    /// if set. Otherwise, for dataset items it is derived from the project,
    /// the dataset and the run name (as the Langfuse server and the other SDKs
    /// do), so runs with the same run name share one experiment; for local
    /// data it is random.
    ///
    /// Returns [`LangfuseError::Config`] for an empty `experiment_id` or a
    /// `max_concurrency` of 0.
    ///
    /// ```rust,no_run
    /// use langfuse_rust_sdk::experiment::{
    ///     Evaluation, EvaluatorInput, ExperimentItem, ExperimentOptions, LocalExperimentItem, evaluator,
    /// };
    /// # async fn ask(_: &serde_json::Value) -> Result<String, std::io::Error> { Ok(String::new()) }
    /// # async fn example(tracer: &langfuse_rust_sdk::otel::LangfuseTracer) -> Result<(), langfuse_rust_sdk::LangfuseError> {
    /// let data = vec![
    ///     LocalExperimentItem::new("Capital of France?").with_expected_output("Paris"),
    ///     LocalExperimentItem::new("Capital of Italy?").with_expected_output("Rome"),
    /// ];
    /// let exact_match = evaluator("exact_match", |args: EvaluatorInput| async move {
    ///     Evaluation::boolean("exact_match", args.expected_output == Some(args.output))
    /// });
    /// let result = tracer
    ///     .run_experiment(
    ///         data,
    ///         |item: ExperimentItem| async move { ask(item.input().unwrap()).await },
    ///         ExperimentOptions::new("capitals").with_evaluator(exact_match),
    ///     )
    ///     .await?;
    /// println!("{result}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_experiment<I, T>(
        &self,
        data: I,
        task: T,
        options: ExperimentOptions,
    ) -> Result<ExperimentResult, LangfuseError>
    where
        I: IntoIterator,
        I::Item: Into<ExperimentItem>,
        T: Task,
    {
        let items = data.into_iter().map(Into::into).collect();
        self.run_items(items, &task, options, None).await
    }

    async fn run_items<T: Task>(
        &self,
        items: Vec<ExperimentItem>,
        task: &T,
        options: ExperimentOptions,
        dataset_version: Option<DateTime<Utc>>,
    ) -> Result<ExperimentResult, LangfuseError> {
        let ExperimentOptions {
            name,
            run_name,
            description,
            metadata,
            max_concurrency,
            experiment_id,
            evaluators,
            composite_evaluator,
            run_evaluators,
        } = options;
        let invalid =
            |msg: &str| LangfuseError::Config(format!("Invalid experiment options: {msg}"));
        if experiment_id
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
        {
            return Err(invalid("experiment_id must be a non-empty string"));
        }
        if max_concurrency == 0 {
            return Err(invalid("max_concurrency must be greater than 0"));
        }
        let run_name = run_name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("{name} - {}", python_isoformat(Utc::now())));
        tracing::debug!(experiment = %name, run = %run_name, items = items.len(), "starting experiment");

        let client = &self.shared().client;
        let project_id = if self.is_enabled() {
            match client.resolve_project_id().await {
                Ok(id) => Some(id),
                Err(err) => {
                    tracing::warn!(error = %err, "could not resolve the project ID for the experiment");
                    None
                }
            }
        } else {
            None
        };
        // One experiment per run: mixed data uses the dataset of the first dataset item.
        let dataset_id = items.iter().find_map(|item| {
            item.dataset_item()
                .map(|d| d.dataset_id.clone())
                .filter(|id| !id.is_empty())
        });
        let experiment_id = experiment_id.unwrap_or_else(|| {
            derive_experiment_id(project_id.as_deref(), dataset_id.as_deref(), &run_name)
        });

        let run = ItemRun {
            tracer: self,
            task,
            evaluators: &evaluators,
            composite_evaluator: composite_evaluator.as_deref(),
            experiment_id: &experiment_id,
            name: &name,
            run_name: &run_name,
            description: description.as_deref(),
            serialized_metadata: (!metadata.is_empty())
                .then(|| python_serialize(&Value::Object(metadata.clone()))),
            metadata,
            item_version: dataset_version.map(python_isoformat),
        };
        let outcomes: Vec<Result<ExperimentItemResult, String>> = stream::iter(items)
            .map(|item| run.process(item))
            .buffered(max_concurrency)
            .collect()
            .await;
        let mut item_results = Vec::with_capacity(outcomes.len());
        for (index, outcome) in outcomes.into_iter().enumerate() {
            match outcome {
                Ok(result) => item_results.push(result),
                Err(err) => tracing::error!(item = index, error = %err, "experiment item failed"),
            }
        }

        let mut run_evaluations = Vec::new();
        if !run_evaluators.is_empty() {
            let input = RunEvaluatorInput::new(item_results);
            for evaluator in &run_evaluators {
                match catch_panic(async { evaluator.evaluate(input.clone()).await }).await {
                    Ok(evaluations) => run_evaluations.extend(evaluations),
                    Err(err) => {
                        tracing::error!(evaluator = evaluator.name(), error = %err, "run evaluator failed");
                    }
                }
            }
            item_results = Arc::unwrap_or_clone(input.item_results);
        }
        // Run scores attach to the experiment through `dataset_run_id`.
        for evaluation in &run_evaluations {
            self.score(CreateScoreRequest {
                dataset_run_id: Some(experiment_id.clone()),
                ..evaluation.to_score_request()
            });
        }

        let experiment_url = project_id.map(|project_id| {
            format!(
                "{}/project/{}/experiments/results?baseline={}",
                client.ui_base_url(),
                encode_path_segment(&project_id),
                encode_path_segment(&experiment_id)
            )
        });
        self.flush().await;
        Ok(ExperimentResult {
            name,
            run_name,
            description,
            item_results,
            run_evaluations,
            experiment_id,
            experiment_url,
        })
    }
}

impl DatasetWithItems {
    /// Run an experiment on the items of this dataset, at the version they
    /// were read at; see [`LangfuseTracer::run_experiment`].
    pub async fn run_experiment<T: Task>(
        &self,
        tracer: &LangfuseTracer,
        task: T,
        options: ExperimentOptions,
    ) -> Result<ExperimentResult, LangfuseError> {
        let items = self.items.iter().cloned().map(Into::into).collect();
        tracer.run_items(items, &task, options, self.version).await
    }
}

/// Settings shared by the items of one run.
struct ItemRun<'a, T> {
    tracer: &'a LangfuseTracer,
    task: &'a T,
    evaluators: &'a [Arc<dyn Evaluator>],
    composite_evaluator: Option<&'a dyn CompositeEvaluator>,
    experiment_id: &'a str,
    name: &'a str,
    run_name: &'a str,
    description: Option<&'a str>,
    metadata: Map<String, Value>,
    serialized_metadata: Option<String>,
    /// Dataset version, recorded on dataset items.
    item_version: Option<String>,
}

impl<T: Task> ItemRun<'_, T> {
    async fn process(&self, item: ExperimentItem) -> Result<ExperimentItemResult, String> {
        let run_span = self
            .tracer
            .start_observation("experiment-item-run", ObservationType::Span);
        let outcome = self
            .run_in_span(&run_span, item)
            .with_context(run_span.context())
            .await;
        if let Err(err) = &outcome {
            mark_failed(&run_span, err);
        }
        run_span.end();
        outcome
    }

    /// Body of [`process`](Self::process), with `run_span` current.
    async fn run_in_span(
        &self,
        run_span: &Observation,
        item: ExperimentItem,
    ) -> Result<ExperimentItemResult, String> {
        let input = item
            .input()
            .cloned()
            .ok_or("Experiment Item is missing input. Skipping item.")?;
        let expected_output = item.expected_output().cloned();
        let item_metadata = item.metadata().cloned();
        let (dataset_id, dataset_item_id) = match item.dataset_item() {
            Some(d) => (Some(d.dataset_id.clone()), Some(d.id.clone())),
            None => (None, None),
        };

        let mut run_metadata = Map::new();
        run_metadata.insert("experiment_name".into(), self.name.into());
        run_metadata.insert("experiment_run_name".into(), self.run_name.into());
        if let (Some(dataset_id), Some(item_id)) = (&dataset_id, &dataset_item_id) {
            run_metadata.insert("dataset_id".into(), dataset_id.clone().into());
            run_metadata.insert("dataset_item_id".into(), item_id.clone().into());
        }
        // Item metadata, then run metadata; the experiment keys win.
        let mut observation_metadata = Map::new();
        if let Some(Value::Object(metadata)) = &item_metadata {
            observation_metadata.extend(metadata.clone());
        }
        observation_metadata.extend(self.metadata.clone());
        observation_metadata.extend(run_metadata);

        let mut direct = vec![KeyValue::new(
            attr::ENVIRONMENT,
            attr::SDK_EXPERIMENT_ENVIRONMENT,
        )];
        if let Some(description) = self.description {
            direct.push(KeyValue::new(
                attr::EXPERIMENT_DESCRIPTION,
                description.to_owned(),
            ));
        }
        if let Some(expected_output) = &expected_output {
            direct.push(KeyValue::new(
                attr::EXPERIMENT_ITEM_EXPECTED_OUTPUT,
                python_serialize(expected_output),
            ));
        }
        if dataset_id.is_some()
            && let Some(version) = &self.item_version
        {
            direct.push(KeyValue::new(
                attr::EXPERIMENT_ITEM_VERSION,
                version.clone(),
            ));
        }
        run_span.buffer(direct.clone());

        let task_span = run_span.start_child("experiment-item-task", ObservationType::Span);
        task_span.set_input(&input);
        task_span.buffer(direct);
        let experiment = ExperimentAttributes {
            id: self.experiment_id.to_owned(),
            name: self.run_name.to_owned(),
            metadata: self.serialized_metadata.clone(),
            dataset_id: dataset_id.clone(),
            item_id: dataset_item_id.unwrap_or_else(|| local_item_id(&input)),
            item_metadata: item_metadata
                .as_ref()
                .filter(|m| m.is_object())
                .map(python_serialize),
            item_root_observation_id: task_span.id().to_owned(),
        };
        // The task span gets the attributes from the scope; the run span is
        // backfilled so it belongs to the experiment too.
        let task_cx = experiment_context(experiment.clone(), &task_span.context());
        run_span.buffer(inherited_attributes(&task_cx));
        task_span.set_metadata(&observation_metadata);

        let task_item = item.clone();
        let outcome = catch_panic(async { self.task.run(task_item).await })
            .with_context(task_cx)
            .await;
        let output = match outcome {
            Ok(output) => output,
            Err(err) => {
                mark_failed(&task_span, &err);
                task_span.end();
                return Err(err);
            }
        };
        task_span.set_output(&output);
        task_span.end();
        run_span
            .set_input(&input)
            .set_output(&output)
            .set_metadata(&observation_metadata);

        let evaluator_input = EvaluatorInput {
            expected_output,
            metadata: item_metadata,
            ..EvaluatorInput::new(input, output.clone())
        };
        let trace_id = run_span.trace_id().to_owned();
        let evaluations = if self.evaluators.is_empty() {
            Vec::new()
        } else {
            let evaluation_cx = experiment_context(experiment, &Context::current());
            self.evaluate(&trace_id, task_span.id(), evaluator_input)
                .with_context(evaluation_cx)
                .await
        };
        Ok(ExperimentItemResult {
            item,
            output,
            evaluations,
            trace_id: run_span.is_recording().then_some(trace_id),
            experiment_id: self.experiment_id.to_owned(),
        })
    }

    /// Run the evaluators of one item under an `experiment-item-evaluation`
    /// span and score their evaluations on the task span.
    async fn evaluate(
        &self,
        trace_id: &str,
        task_span_id: &str,
        input: EvaluatorInput,
    ) -> Vec<Evaluation> {
        let evaluation_span = self
            .tracer
            .start_observation("experiment-item-evaluation", ObservationType::Span);
        let scope = evaluation_span.context();
        let (evaluations, failed) = async {
            let mut evaluations = Vec::new();
            let mut failed = 0usize;
            for (index, evaluator) in self.evaluators.iter().enumerate() {
                let span = self
                    .tracer
                    .start_observation(evaluator.name(), ObservationType::Evaluator);
                span.set_input(&input).set_metadata(&json!({
                    "evaluator_kind": "item",
                    "evaluator_index": index,
                }));
                let evaluated = span.in_scope(catch_panic(async {
                    evaluator.evaluate(input.clone()).await
                }));
                match evaluated.await {
                    Ok(results) => {
                        span.set_output(&results);
                        span.end();
                        for evaluation in &results {
                            self.score(trace_id, task_span_id, evaluation);
                        }
                        evaluations.extend(results);
                    }
                    Err(err) => {
                        failed += 1;
                        record_evaluator_error(&span, &err);
                        tracing::error!(evaluator = evaluator.name(), error = %err, "evaluator failed");
                    }
                }
            }
            if let Some(composite) = self.composite_evaluator
                && !evaluations.is_empty()
            {
                let composite_input = CompositeEvaluatorInput::new(input, evaluations.clone());
                let span = self
                    .tracer
                    .start_observation(composite.name(), ObservationType::Evaluator);
                span.set_input(&composite_input)
                    .set_metadata(&json!({"evaluator_kind": "composite"}));
                let evaluated = span.in_scope(catch_panic(async {
                    composite.evaluate(composite_input).await
                }));
                match evaluated.await {
                    Ok(results) => {
                        span.set_output(&results);
                        span.end();
                        for evaluation in results {
                            self.score(trace_id, task_span_id, &evaluation);
                            evaluations.push(evaluation);
                        }
                    }
                    Err(err) => {
                        failed += 1;
                        record_evaluator_error(&span, &err);
                        tracing::error!(evaluator = composite.name(), error = %err, "composite evaluator failed");
                    }
                }
            }
            (evaluations, failed)
        }
        .with_context(scope)
        .await;

        let has_composite = self.composite_evaluator.is_some();
        evaluation_span.set_output(&json!({
            "evaluator_count": self.evaluators.len() + usize::from(has_composite),
            "evaluation_count": evaluations.len(),
            "failed_evaluator_count": failed,
            "skipped_evaluator_count": usize::from(has_composite && evaluations.is_empty()),
        }));
        evaluation_span.end();
        evaluations
    }

    fn score(&self, trace_id: &str, task_span_id: &str, evaluation: &Evaluation) {
        self.tracer.score(CreateScoreRequest {
            trace_id: Some(trace_id.to_owned()),
            observation_id: Some(task_span_id.to_owned()),
            ..evaluation.to_score_request()
        });
    }
}

fn mark_failed(span: &Observation, error: &str) {
    span.set_output(&format!("Error: {error}"))
        .set_level(Level::Error)
        .set_status_message(error);
}

fn record_evaluator_error(span: &Observation, error: &str) {
    span.set_output(&json!({"error": error}))
        .set_level(Level::Error)
        .set_status_message(error);
    span.end();
}

/// The experiment ID when none is given: for dataset items with a known
/// project, the first 16 hex characters of the SHA-256 of
/// `["langfuse-experiment-v1", project_id, dataset_id, run_name]` as compact
/// JSON (the derivation of the Langfuse server); otherwise random.
fn derive_experiment_id(
    project_id: Option<&str>,
    dataset_id: Option<&str>,
    run_name: &str,
) -> String {
    match (project_id, dataset_id) {
        (Some(project_id), Some(dataset_id)) => {
            let payload = serde_json::to_string(&[
                "langfuse-experiment-v1",
                project_id,
                dataset_id,
                run_name,
            ])
            .expect("strings serialize");
            seeded_hex(&payload, 8)
        }
        (None, Some(_)) => {
            tracing::warn!(
                run = run_name,
                "unknown project ID; using a random experiment ID, so re-runs with the same run name are not grouped"
            );
            create_observation_id(None)
        }
        (_, None) => create_observation_id(None),
    }
}

/// Item ID of local data: the first 16 hex characters of the SHA-256 of the
/// input as the Python SDK serializes it.
fn local_item_id(input: &Value) -> String {
    seeded_hex(&python_serialize(input), 8)
}

/// Strings as-is, anything else as Python's `json.dumps` writes it (`", "`
/// and `": "` separators, non-ASCII escaped, unsafe integers as strings).
///
/// Object keys come out sorted (as `serde_json::Value` stores them), so an
/// object whose keys were not in alphabetical order hashes differently than
/// in Python.
fn python_serialize(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => {
            let mut out = Vec::new();
            let mut serializer = serde_json::Serializer::with_formatter(&mut out, PythonFormatter);
            js_safe(other.clone())
                .serialize(&mut serializer)
                .expect("JSON values serialize");
            String::from_utf8(out).expect("the formatter writes ASCII")
        }
    }
}

/// Formatter matching Python's `json.dumps` defaults (`ensure_ascii=True`).
struct PythonFormatter;

impl serde_json::ser::Formatter for PythonFormatter {
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }

    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        writer.write_all(python_float_repr(value).as_bytes())
    }

    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut units = [0u16; 2];
        for c in fragment.chars() {
            if c.is_ascii() && c != '\x7f' {
                writer.write_all(&[c as u8])?;
            } else {
                for unit in c.encode_utf16(&mut units) {
                    write!(writer, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experiment_ids_match_the_server_derivation() {
        assert_eq!(
            derive_experiment_id(Some("p"), Some("d"), "r"),
            "eab007015b1c6f77"
        );
        assert_eq!(
            derive_experiment_id(
                Some("7a88fb47-b4e2-43b8-a06c-a5ce950dc53a"),
                Some("cm9x1dataset0000000000001"),
                "my-run"
            ),
            "a1164c0f238e4173"
        );
        let random = derive_experiment_id(Some("p"), None, "r");
        assert_eq!(random.len(), 16);
        assert_ne!(random, derive_experiment_id(Some("p"), None, "r"));
        assert_eq!(derive_experiment_id(None, Some("d"), "r").len(), 16);
    }

    #[test]
    fn values_serialize_like_python_json_dumps() {
        assert_eq!(python_serialize(&json!("plain")), "plain");
        // json.dumps({"a": "é😀\x7f\n\"x\"", "b": [1, 2.5, True, None]})
        assert_eq!(
            python_serialize(&json!({"b": [1, 2.5, true, null], "a": "é😀\u{7f}\n\"x\""})),
            r#"{"a": "\u00e9\ud83d\ude00\u007f\n\"x\"", "b": [1, 2.5, true, null]}"#
        );
        assert_eq!(python_serialize(&json!([])), "[]");
        assert_eq!(python_serialize(&json!({})), "{}");
        assert_eq!(python_serialize(&json!(1e-7)), "1e-07");
        assert_eq!(
            python_serialize(&json!(9_007_199_254_740_993_u64)),
            "\"9007199254740993\""
        );
    }

    #[test]
    fn local_item_ids_hash_the_python_serialization() {
        // hashlib.sha256(json.dumps({"q": "Capital of France?"}).encode()).hexdigest()[:16]
        assert_eq!(
            local_item_id(&json!({"q": "Capital of France?"})),
            "a1139c1d53c78d7a"
        );
        // Keys in alphabetical order hash as in Python, non-ASCII included.
        assert_eq!(
            local_item_id(&json!({"n": 1.0, "q": "¿Capital de España?"})),
            "032bbeda5b7074ba"
        );
        // hashlib.sha256(b"hello").hexdigest()[:16]
        assert_eq!(local_item_id(&json!("hello")), "2cf24dba5fb0a30e");
    }
}
